//! Defines the interface which is implemented by the backend modules

cfg_select! {
    all(feature = "backend_fjall", feature = "backend_sled") => {
        compile_error!("You cannot have both database backend features active at the same time. You must choose either \"backend_fjall\" or \"backend_sled\".");
    }
    feature = "backend_fjall" => {
        mod fjall;
        use fjall as backend;
    }
    feature = "backend_sled" => {
        mod sled;
        use sled as backend;
    }
    _ => {
        compile_error!("Neither database backend feature is active. You must choose either \"backend_fjall\" or \"backend_sled\".");
    }
}

use std::path::Path;

pub use backend::{Buffer, Builder, Database, Error, Iter, Result, Table, Transaction};

pub trait BuilderImpl {
    type Database;
    fn new_with_path(path: impl AsRef<Path>) -> Self;
    fn cache_size(self, bytes: u64) -> Self;
    fn compression(self, yes: bool) -> Self;
    fn temporary(self, yes: bool) -> Self;
    fn open(self) -> Result<Self::Database>;
}

pub trait DatabaseImpl {
    type Table;
    type Transaction<'a>
    where
        Self: 'a;
    fn open_table(&self, name: &str) -> Result<Self::Table>;
    fn transaction(
        &self,
        f: impl Fn(Self::Transaction<'_>) -> Result<TransactionResult>,
    ) -> Result<()>;
}

pub trait TableImpl {
    type Iter;
    fn get(&self, key: impl AsRef<[u8]>) -> Result<Option<Buffer>>;
    fn insert(&self, key: impl AsRef<[u8]>, value: impl Into<Buffer>) -> Result<()>;
    fn remove(&self, key: impl AsRef<[u8]>) -> Result<()>;
    fn first_kv(&self) -> Result<Option<(Buffer, Buffer)>>;
    fn last_kv(&self) -> Result<Option<(Buffer, Buffer)>>;
    fn prefix(&self, prefix: impl AsRef<[u8]>) -> Self::Iter;
}

pub trait IterImpl: Iterator<Item = Result<(Buffer, Buffer)>> + DoubleEndedIterator {}

/// This type cannot be constructed outside of this module, so the only way to obtain
/// an instance of it is by calling `commit` or `rollback`, which both consume `self`
pub struct TransactionResult(TransactionResultType);

/// This type and the above tuple struct field should be private so that `TransactionResult` cannot be constructed outside this module
enum TransactionResultType {
    Commit,
    Rollback,
}

/// Dropping the transaction type should default to rollback
pub trait TransactionImpl: Sized {
    type Table;

    fn get(&self, table: &Self::Table, key: impl Into<Buffer>) -> Result<Option<Buffer>>;

    fn insert(
        &mut self,
        table: &Self::Table,
        key: impl Into<Buffer>,
        value: impl Into<Buffer>,
    ) -> Result<()>;

    fn remove(&mut self, table: &Self::Table, key: impl Into<Buffer>) -> Result<()>;

    fn commit(self) -> Result<TransactionResult> {
        Ok(TransactionResult(TransactionResultType::Commit))
    }

    fn rollback(self) -> TransactionResult {
        TransactionResult(TransactionResultType::Rollback)
    }
}
