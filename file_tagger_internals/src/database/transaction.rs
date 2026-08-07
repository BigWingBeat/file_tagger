use std::{
    sync::mpsc::{Receiver, SyncSender},
    thread::JoinHandle,
};

use super::{
    Buffer, Database, DatabaseImpl, Result, TransactionImpl, TransactionResult, UntypedIter,
    UntypedTable,
};

/// This is the message type passed over a channel to allow the GUI to control the transaction
enum TransAction {
    Get(UntypedTable, Buffer),
    Insert(UntypedTable, Buffer, Buffer),
    Remove(UntypedTable, Buffer),
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
    Prefix(UntypedIter),
    Commit(Result<()>),
}

/// Talks to the transaction thread
/// Doing the transaction stuff on another thread is necessary to workaround quirks in the backend APIs
#[derive(Debug)]
pub struct TransactionApi {
    sender: SyncSender<TransAction>,
    receiver: Receiver<TransReaction>,
    thread_handle: Option<JoinHandle<()>>,
}

impl TransactionApi {
    pub fn get(&self, table: &UntypedTable, key: impl Into<Buffer>) -> Result<Option<Buffer>> {
        self.sender
            .send(TransAction::Get(table.clone(), key.into()))
            .unwrap();
        let TransReaction::Get(result) = self.receiver.recv().unwrap() else {
            unreachable!();
        };
        result
    }

    pub fn insert(
        &mut self,
        table: &UntypedTable,
        key: impl Into<Buffer>,
        value: impl Into<Buffer>,
    ) -> Result<()> {
        self.sender
            .send(TransAction::Insert(table.clone(), key.into(), value.into()))
            .unwrap();
        let TransReaction::Insert(result) = self.receiver.recv().unwrap() else {
            unreachable!();
        };
        result
    }

    pub fn remove(&mut self, table: &UntypedTable, key: impl Into<Buffer>) -> Result<()> {
        self.sender
            .send(TransAction::Remove(table.clone(), key.into()))
            .unwrap();
        let TransReaction::Remove(result) = self.receiver.recv().unwrap() else {
            unreachable!();
        };
        result
    }

    pub fn prefix(&self, table: &UntypedTable, prefix: impl Into<Buffer>) -> UntypedIter {
        self.sender
            .send(TransAction::Prefix(table.clone(), prefix.into()))
            .unwrap();
        let TransReaction::Prefix(iter) = self.receiver.recv().unwrap() else {
            unreachable!()
        };
        iter
    }

    pub fn commit(self) -> Result<()> {
        self.sender.send(TransAction::Commit).unwrap();
        let TransReaction::Commit(result) = self.receiver.recv().unwrap() else {
            unreachable!();
        };
        result
    }

    pub fn rollback(self) {
        self.sender.send(TransAction::Rollback).unwrap();
    }
}

impl Drop for TransactionApi {
    fn drop(&mut self) {
        // Failsafe in case the transaction was dropped without being finalized
        let _ = self.sender.send(TransAction::Rollback);
        self.thread_handle.take().unwrap().join().unwrap();
    }
}

pub fn initialize_transaction(db: Database) -> TransactionApi {
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
                match action {
                    TransAction::Get(table, key) => {
                        sender
                            .send(TransReaction::Get(transaction.get(&table, key)))
                            .unwrap();
                    }
                    TransAction::Insert(table, key, value) => {
                        sender
                            .send(TransReaction::Insert(
                                transaction.insert(&table, key, value),
                            ))
                            .unwrap();
                    }
                    TransAction::Remove(table, key) => {
                        sender
                            .send(TransReaction::Remove(transaction.remove(&table, key)))
                            .unwrap();
                    }
                    TransAction::Prefix(table, prefix) => {
                        sender
                            .send(TransReaction::Prefix(transaction.prefix(&table, prefix)))
                            .unwrap();
                    }
                    TransAction::Commit => return transaction.commit(),
                    TransAction::Rollback => break,
                }
            }
            // Either we explicitly received a `Rollback` action, or the sender was disconnected
            Ok(transaction.rollback())
        });
        // The results of all non-finalizing actions get sent over the channel, so `Ok` and `Err` here are always from a `Commit`
        match result {
            TransactionResult::Ok(()) => sender.send(TransReaction::Commit(Ok(()))).unwrap(),
            TransactionResult::Err(e) => sender.send(TransReaction::Commit(Err(e))).unwrap(),
            TransactionResult::Rollback => {}
        }
    });

    TransactionApi {
        sender: action_sender,
        receiver: reaction_receiver,
        thread_handle: Some(thread),
    }
}
