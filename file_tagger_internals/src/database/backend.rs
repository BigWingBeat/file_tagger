//! Defines the interface which is implemented by the backend modules

mod sqlite;

use std::path::Path;

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
    type Transaction;
    fn open_table(&mut self, name: &str) -> Result<Self::Table>;
    fn transaction(&mut self) -> Result<Self::Transaction>;
}

pub trait TableImpl {
    type Iter;
    fn get(&self, key: impl AsRef<[u8]>) -> Result<Option<Buffer>>;
    fn insert(&mut self, key: impl AsRef<[u8]>, value: impl Into<Buffer>) -> Result<()>;
    fn remove(&mut self, key: impl AsRef<[u8]>) -> Result<()>;
    fn take(&mut self, key: impl AsRef<[u8]>) -> Result<Option<Buffer>>;
    fn first_kv(&self) -> Result<Option<(Buffer, Buffer)>>;
    fn last_kv(&self) -> Result<Option<(Buffer, Buffer)>>;
    fn prefix(&self, prefix: impl AsRef<[u8]>) -> Self::Iter;

    fn fetch_update<F>(&mut self, key: impl Into<Buffer>, f: F) -> Result<Option<Buffer>>
    where
        F: FnMut(Option<&Buffer>) -> Option<Buffer>;
}

pub trait IterImpl: Iterator<Item = Result<(Buffer, Buffer)>> + DoubleEndedIterator {}

/// Dropping the transaction type should default to rollback
pub trait TransactionImpl: Sized {
    type Table;
    type Iter;
    type Conflict;

    fn get(&self, table: &Self::Table, key: impl Into<Buffer>) -> Result<Option<Buffer>>;

    fn insert(
        &mut self,
        table: &mut Self::Table,
        key: impl Into<Buffer>,
        value: impl Into<Buffer>,
    ) -> Result<()>;

    fn remove(&mut self, table: &mut Self::Table, key: impl Into<Buffer>) -> Result<()>;
    fn take(&mut self, table: &mut Self::Table, key: impl Into<Buffer>) -> Result<Option<Buffer>>;
    fn first_kv(&self, table: &Self::Table) -> Result<Option<(Buffer, Buffer)>>;
    fn last_kv(&self, table: &Self::Table) -> Result<Option<(Buffer, Buffer)>>;
    fn prefix(&self, table: &Self::Table, prefix: impl AsRef<[u8]>) -> Self::Iter;

    fn fetch_update<F>(
        &mut self,
        table: &mut Self::Table,
        key: impl Into<Buffer>,
        f: F,
    ) -> Result<Option<Buffer>>
    where
        F: FnOnce(Option<&Buffer>) -> Option<Buffer>;

    fn commit(self) -> Result<Result<(), Self::Conflict>>;
    fn rollback(self);
}
