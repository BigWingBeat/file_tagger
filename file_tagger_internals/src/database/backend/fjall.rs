use std::path::Path;

use fjall::{CompressionType, KeyspaceCreateOptions, Readable};

pub use fjall::Conflict;

pub type Error = fjall::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;

pub type Buffer = fjall::Slice;

/// Non-transactional impl
mod NoTx {
    pub type FjallDatabase = fjall::Database;
    pub type Keyspace = fjall::Keyspace;
    // No single transaction type, instead there's `Snapshot` which is read-only and `WriteBatch` which is write-only
}

/// Single-writer transactional impl (mutex locks the db)
mod SingleWriterTx {
    pub type FjallDatabase = fjall::SingleWriterTxDatabase;
    pub type Keyspace = fjall::SingleWriterTxKeyspace;
    pub type FjallTransaction<'a> = fjall::SingleWriterWriteTx<'a>;
}

/// Multi-writer transactional impl (optimistic concurrency control)
mod OptimisticTx {
    pub type FjallDatabase = fjall::OptimisticTxDatabase;
    pub type Keyspace = fjall::OptimisticTxKeyspace;
    pub type FjallTransaction = fjall::OptimisticWriteTx;
}

/// Choose one of the three database impls:
/// - `NoTx` doesn't support read-what-you-write transactions, which we need
/// - `SingleWriterTx` requires Shenanigans to workaround lifetime issues
/// - `OptimisticTx` has the nicest API, but worse performance, as we have no concurrency
///
/// Ideally we would use `NoTx` with `BaseTransaction`, but the latter is not exposed anywhere.
/// We use `OptimisticTx` as it is the only type that actually provides the API and feature set we need,
/// so we just have to eat the performance hit.
///
/// See: <https://github.com/fjall-rs/fjall/issues/318>
// use NoTx::{FjallDatabase, Keyspace};
// use SingleWriterTx::{FjallDatabase, FjallTransaction, Keyspace};
use OptimisticTx::{FjallDatabase, FjallTransaction, Keyspace};

#[repr(transparent)]
pub struct Builder(fjall::DatabaseBuilder<FjallDatabase>);

impl super::BuilderImpl for Builder {
    type Database = Database;

    #[inline(always)]
    fn new_with_path(path: impl AsRef<Path>) -> Self {
        Self(FjallDatabase::builder(path))
    }

    #[inline(always)]
    fn cache_size(self, bytes: u64) -> Self {
        Self(self.0.cache_size(bytes))
    }

    #[inline(always)]
    fn compression(self, yes: bool) -> Self {
        let comp = if yes {
            CompressionType::Lz4
        } else {
            CompressionType::None
        };
        Self(self.0.journal_compression(comp))
    }

    #[inline(always)]
    fn temporary(self, yes: bool) -> Self {
        Self(self.0.temporary(yes))
    }

    #[inline(always)]
    fn open(self) -> Result<Self::Database> {
        self.0.open().map(Database)
    }
}

#[derive(Clone)]
#[repr(transparent)]
pub struct Database(FjallDatabase);

impl super::DatabaseImpl for Database {
    type Table = Table;
    type Transaction = Transaction;

    #[inline(always)]
    fn open_table(&mut self, name: &str) -> Result<Self::Table> {
        self.0
            .keyspace(name, KeyspaceCreateOptions::default)
            .map(Table)
    }

    fn transaction(&mut self) -> Result<Self::Transaction> {
        self.0.write_tx().map(Transaction)
    }
}

#[derive(Clone)]
#[repr(transparent)]
pub struct Table(Keyspace);

impl super::TableImpl for Table {
    type Iter = Iter;

    #[inline(always)]
    fn get(&self, key: impl AsRef<[u8]>) -> Result<Option<Buffer>> {
        self.0.get(key)
    }

    #[inline(always)]
    fn insert(&mut self, key: impl AsRef<[u8]>, value: impl Into<Buffer>) -> Result<()> {
        self.0.insert(key.as_ref(), value)
    }

    fn remove(&mut self, key: impl AsRef<[u8]>) -> Result<()> {
        self.0.remove(key.as_ref())
    }

    fn take(&mut self, key: impl AsRef<[u8]>) -> Result<Option<Buffer>> {
        self.0.take(key.as_ref())
    }

    #[inline(always)]
    fn first_kv(&self) -> Result<Option<(Buffer, Buffer)>> {
        self.0
            .first_key_value()
            .map(|guard| guard.into_inner())
            .transpose()
    }

    #[inline(always)]
    fn last_kv(&self) -> Result<Option<(Buffer, Buffer)>> {
        self.0
            .last_key_value()
            .map(|guard| guard.into_inner())
            .transpose()
    }

    #[inline(always)]
    fn prefix(&self, prefix: impl AsRef<[u8]>) -> Iter {
        Iter(self.0.as_ref().prefix(prefix))
    }

    fn fetch_update<F>(&mut self, key: impl Into<Buffer>, f: F) -> Result<Option<Buffer>>
    where
        F: FnMut(Option<&Buffer>) -> Option<Buffer>,
    {
        self.0.fetch_update(key, f)
    }
}

#[repr(transparent)]
pub struct Iter(fjall::Iter);

impl Iterator for Iter {
    type Item = Result<(Buffer, Buffer)>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|guard| guard.into_inner())
    }
}

impl DoubleEndedIterator for Iter {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(|guard| guard.into_inner())
    }
}

impl super::IterImpl for Iter {}

#[repr(transparent)]
pub struct Transaction(FjallTransaction);

impl Transaction {
    fn new(transaction: FjallTransaction) -> Self {
        Self(transaction)
    }
}

impl super::TransactionImpl for Transaction {
    type Table = Table;
    type Iter = Iter;
    type Conflict = Conflict;

    fn get(&self, table: &Self::Table, key: impl Into<Buffer>) -> Result<Option<Buffer>> {
        self.0.get(&table.0, key.into())
    }

    fn insert(
        &mut self,
        table: &mut Self::Table,
        key: impl Into<Buffer>,
        value: impl Into<Buffer>,
    ) -> Result<()> {
        self.0.insert(&table.0, key, value);
        Ok(())
    }

    fn remove(&mut self, table: &mut Self::Table, key: impl Into<Buffer>) -> Result<()> {
        self.0.remove(&table.0, key);
        Ok(())
    }

    fn take(&mut self, table: &mut Self::Table, key: impl Into<Buffer>) -> Result<Option<Buffer>> {
        self.0.take(&table.0, key)
    }

    fn first_kv(&self, table: &Self::Table) -> Result<Option<(Buffer, Buffer)>> {
        self.0
            .first_key_value(&table.0)
            .map(|guard| guard.into_inner())
            .transpose()
    }

    fn last_kv(&self, table: &Self::Table) -> Result<Option<(Buffer, Buffer)>> {
        self.0
            .last_key_value(&table.0)
            .map(|guard| guard.into_inner())
            .transpose()
    }

    fn prefix(&self, table: &Self::Table, prefix: impl AsRef<[u8]>) -> Self::Iter {
        Iter(self.0.prefix(&table.0, prefix))
    }

    fn fetch_update<F>(
        &mut self,
        table: &mut Self::Table,
        key: impl Into<Buffer>,
        f: F,
    ) -> Result<Option<Buffer>>
    where
        F: FnOnce(Option<&Buffer>) -> Option<Buffer>,
    {
        self.0.fetch_update(&table.0, key, f)
    }

    fn commit(self) -> Result<Result<(), Self::Conflict>> {
        self.0.commit()
    }

    fn rollback(self) {
        self.0.rollback()
    }
}
