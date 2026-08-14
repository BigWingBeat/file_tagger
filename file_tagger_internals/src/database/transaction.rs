use std::{
    sync::mpsc::{Receiver, SyncSender},
    thread::JoinHandle,
};

use super::{
    Buffer, Result, UntypedIter, UntypedTable,
    backend::{Database, DatabaseImpl, TransactionImpl, TransactionResult},
};

/// This is the message type passed over a channel to allow the GUI to control the transaction
enum TransAction {
    Get(UntypedTable, Buffer),
    Insert(UntypedTable, Buffer, Buffer),
    Remove(UntypedTable, Buffer),
    FirstKv(UntypedTable),
    LastKv(UntypedTable),
    Prefix(UntypedTable, Buffer),
    Commit,
    Rollback,
}

/// This is the message type passed back from the transaction thread with the result of the action.
/// There is no corresponding `Rollback` variant because that is an infallible operation
enum TransReaction {
    Get(Result<Option<Buffer>>),
    Insert(Result<()>),
    Remove(Result<()>),
    FirstKv(Result<Option<(Buffer, Buffer)>>),
    LastKv(Result<Option<(Buffer, Buffer)>>),
    Prefix(UntypedIter),
    Commit(Result<()>),
}

/// Talks to the transaction thread
/// Doing the transaction stuff on another thread is necessary to workaround quirks in the backend APIs
#[derive(Debug)]
pub struct Transaction {
    sender: SyncSender<TransAction>,
    receiver: Receiver<TransReaction>,
    thread_handle: Option<JoinHandle<()>>,
}

macro_rules! transaction_method {
    ($self:ident, $operation:ident, $($params:expr),* $(,)*) => {
        transaction_method!($self, $operation = TransAction::$operation($($params),*))
    };
    ($self:ident, $operation:ident) => {
        transaction_method!($self, $operation = TransAction::$operation)
    };
    ($self:ident, $operation:ident = $action:expr) => {{
        $self.sender.send($action).expect("Transaction thread disconnected before send");
        let TransReaction::$operation(result) = $self.receiver.recv().expect("Transaction thread disconnected before recv") else {
            unreachable!("Transaction thread returned incorrect variant");
        };
        result
    }};
}

impl Transaction {
    pub fn get(&self, table: &UntypedTable, key: impl Into<Buffer>) -> Result<Option<Buffer>> {
        transaction_method!(self, Get, table.clone(), key.into())
    }

    pub fn insert(
        &mut self,
        table: &UntypedTable,
        key: impl Into<Buffer>,
        value: impl Into<Buffer>,
    ) -> Result<()> {
        transaction_method!(self, Insert, table.clone(), key.into(), value.into())
    }

    pub fn remove(&mut self, table: &UntypedTable, key: impl Into<Buffer>) -> Result<()> {
        transaction_method!(self, Remove, table.clone(), key.into())
    }

    pub fn first_kv(&self, table: &UntypedTable) -> Result<Option<(Buffer, Buffer)>> {
        transaction_method!(self, FirstKv, table.clone())
    }

    pub fn last_kv(&self, table: &UntypedTable) -> Result<Option<(Buffer, Buffer)>> {
        transaction_method!(self, LastKv, table.clone())
    }

    pub fn prefix(&self, table: &UntypedTable, prefix: impl Into<Buffer>) -> UntypedIter {
        transaction_method!(self, Prefix, table.clone(), prefix.into())
    }

    pub fn commit(self) -> Result<()> {
        transaction_method!(self, Commit)
    }

    pub fn rollback(self) {
        self.sender
            .send(TransAction::Rollback)
            .expect("Transaction thread disconnected before send");
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        // Failsafe in case the transaction was dropped without being finalized
        let _ = self.sender.send(TransAction::Rollback);
        self.thread_handle
            .take()
            .unwrap()
            .join()
            .expect("Transaction thread panicked");
    }
}

pub fn initialize_transaction(db: Database) -> Transaction {
    // Capacities of 0 because we always wait to get a result back right after sending an action,
    // so it's not possible for multiple messages to get queued up on either channel
    let (action_sender, action_receiver) = std::sync::mpsc::sync_channel(0);
    let (reaction_sender, reaction_receiver) = std::sync::mpsc::sync_channel(0);
    let thread = std::thread::spawn(move || {
        let receiver = action_receiver;
        let sender = reaction_sender;
        let result = db.transaction(|mut transaction| {
            // This must always send a `TransReaction` back after each received `TransAction`, otherwise it will deadlock
            // (Except for `rollback` which is infallible and thus has no result to return)
            while let Ok(action) = receiver.recv() {
                macro_rules! transaction_match {
                    ($($variant:ident($($field:ident),*) = $fn:ident),* $(,)*) => {
                        match action {
                            $(
                                TransAction::$variant(ref table, $($field),*) => {
                                    sender
                                        .send(TransReaction::$variant(transaction.$fn(table, $($field),*)))
                                        .expect("Main thread disconnected before send");
                                }
                            ),*
                            TransAction::Commit => return transaction.commit(),
                            TransAction::Rollback => break,
                        }
                    };
                }

                transaction_match!(
                    Get(key) = get,
                    Insert(key, value) = insert,
                    Remove(key) = remove,
                    FirstKv() = first_kv,
                    LastKv() = last_kv,
                    Prefix(prefix) = prefix,
                )
            }
            // Either we explicitly received a `Rollback` action, or the sender was disconnected
            Ok(transaction.rollback())
        });
        // The results of all non-finalizing actions get sent over the channel, so `Ok` and `Err` here are always from a `Commit`
        match result {
            TransactionResult::Ok(()) => sender
                .send(TransReaction::Commit(Ok(())))
                .expect("Main thread disconnected before send"),
            TransactionResult::Err(e) => sender
                .send(TransReaction::Commit(Err(e)))
                .expect("Main thread disconnected before send"),
            TransactionResult::Rollback => {}
        }
    });

    Transaction {
        sender: action_sender,
        receiver: reaction_receiver,
        thread_handle: Some(thread),
    }
}
