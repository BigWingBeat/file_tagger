//! Defines the interface which is implemented by the backend modules

cfg_select! {
    feature = "backend_fjall" => {
        mod fjall;
        use fjall as backend;
    }
    feature = "backend_sled" => {
        mod sled;
        use sled as backend;
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
    fn transaction(&self) -> Result<Self::Transaction<'_>>;
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

/// Dropping the transaction type should default to rollback
pub trait TransactionImpl {
    fn get(&self, key: impl Into<Buffer>) -> Result<Option<Buffer>>;
    fn insert(&mut self, key: impl Into<Buffer>, value: impl Into<Buffer>) -> Result<()>;
    fn remove(&mut self, key: impl Into<Buffer>) -> Result<()>;
    fn commit(self) -> Result<()>;
    fn rollback(self);
}
