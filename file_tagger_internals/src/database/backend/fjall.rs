use std::path::Path;

use fjall::{CompressionType, KeyspaceCreateOptions, Readable};

use super::{FinalizeTransaction, FinalizeTransactionType, TransactionResult};

pub type Error = fjall::Error;

pub type Result<T> = fjall::Result<T>;

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
/// We use `SingleWriterTx` as the aforementioned shenanigans are *mandatory* for the corresponding Sled impl,
/// so as we are already paying that cost, this `SingleWriterTx` impl is essentially free
// use NoTx::{FjallDatabase, Keyspace};
use SingleWriterTx::{FjallDatabase, FjallTransaction, Keyspace};
// use OptimisticTx::{FjallDatabase, FjallTransaction, Keyspace};

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
    type Transaction<'a> = Transaction<'a>;

    #[inline(always)]
    fn open_table(&self, name: &str) -> Result<Self::Table> {
        self.0
            .keyspace(name, KeyspaceCreateOptions::default)
            .map(Table)
    }

    fn transaction(
        &self,
        f: impl Fn(Self::Transaction<'_>) -> Result<super::FinalizeTransaction>,
    ) -> TransactionResult {
        let transaction = self.0.write_tx();
        let result = f(Transaction(transaction));
        match result {
            Ok(FinalizeTransaction(FinalizeTransactionType::Commit)) => TransactionResult::Ok(()),
            Ok(FinalizeTransaction(FinalizeTransactionType::Rollback)) => {
                TransactionResult::Rollback
            }
            Err(e) => TransactionResult::Err(e),
        }
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
    fn insert(&self, key: impl AsRef<[u8]>, value: impl Into<Buffer>) -> Result<()> {
        self.0.insert(key.as_ref(), value)
    }

    fn remove(&self, key: impl AsRef<[u8]>) -> Result<()> {
        self.0.remove(key.as_ref())
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
pub struct Transaction<'a>(FjallTransaction<'a>);

impl<'a> Transaction<'a> {
    fn new(transaction: FjallTransaction<'a>) -> Self {
        Self(transaction)
    }
}

impl super::TransactionImpl for Transaction<'_> {
    type Table = Table;
    type Iter = Iter;

    fn get(&self, table: &Self::Table, key: impl Into<Buffer>) -> Result<Option<Buffer>> {
        self.0.get(&table.0, key.into())
    }

    fn insert(
        &mut self,
        table: &Self::Table,
        key: impl Into<Buffer>,
        value: impl Into<Buffer>,
    ) -> Result<()> {
        self.0.insert(&table.0, key, value);
        Ok(())
    }

    fn remove(&mut self, table: &Self::Table, key: impl Into<Buffer>) -> Result<()> {
        self.0.remove(&table.0, key);
        Ok(())
    }

    fn prefix(&self, table: &Self::Table, prefix: impl AsRef<[u8]>) -> Self::Iter {
        Iter(self.0.prefix(&table.0, prefix))
    }

    fn commit(self) -> Result<FinalizeTransaction> {
        self.0
            .commit()
            .map(|_| FinalizeTransaction(FinalizeTransactionType::Commit))
    }

    fn rollback(self) -> FinalizeTransaction {
        self.0.rollback();
        FinalizeTransaction(FinalizeTransactionType::Rollback)
    }
}
