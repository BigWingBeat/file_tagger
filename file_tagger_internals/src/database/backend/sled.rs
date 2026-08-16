use std::path::Path;

use sled::{
    Transactional,
    transaction::{
        ConflictableTransactionError, TransactionError, TransactionalTree,
        UnabortableTransactionError,
    },
};

use super::{FinalizeTransaction, FinalizeTransactionType, TransactionResult};

pub type Error = sled::Error;

pub type Result<T> = sled::Result<T>;

pub type Buffer = sled::IVec;

#[repr(transparent)]
pub struct Builder(sled::Config);

impl super::BuilderImpl for Builder {
    type Database = Database;

    #[inline(always)]
    fn new_with_path(path: impl AsRef<Path>) -> Self {
        Self(sled::Config::new().path(path).mode(sled::Mode::LowSpace))
    }

    #[inline(always)]
    fn cache_size(self, bytes: u64) -> Self {
        Self(self.0.cache_capacity(bytes))
    }

    #[inline(always)]
    fn compression(self, yes: bool) -> Self {
        Self(self.0.use_compression(yes))
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
pub struct Database(sled::Db);

impl super::DatabaseImpl for Database {
    type Table = Table;

    type Transaction<'a> = Transaction<'a>;

    #[inline(always)]
    fn open_table(&mut self, name: &str) -> Result<Self::Table> {
        self.0.open_tree(name).map(Table)
    }

    fn transaction(
        &mut self,
        f: impl Fn(Self::Transaction<'_>) -> Result<FinalizeTransaction>,
    ) -> TransactionResult {
        // Sled requires us to specify all of the trees involved in a transaction upfront,
        // whereas Fjall allows specifying any keyspace for each operation in the transaction.
        // To emulate the latter with the former, we initiate the transaction with *every* tree in the database.
        // Sled's trees are not `PartialEq` though, so we use the "name" of the trees as the key for comparisons.
        let names = self.0.tree_names();
        // `try_collect` is unstable
        let trees = match names.iter().try_fold(Vec::new(), |mut vec, name| {
            self.0.open_tree(name).map(|tree| {
                vec.push(tree);
                vec
            })
        }) {
            Ok(trees) => trees,
            Err(e) => return TransactionResult::Err(e),
        };
        match trees.transaction(|trees| {
            let result = f(Transaction::new(trees, &names));
            match result {
                // Returning `Ok` signals for the transaction to be committed
                Ok(FinalizeTransaction(FinalizeTransactionType::Commit)) => Ok(()),
                // Sled calls a transaction rollback an "abort"
                Ok(FinalizeTransaction(FinalizeTransactionType::Rollback)) => {
                    Err(ConflictableTransactionError::Abort(()))
                }
                Err(e) => Err(ConflictableTransactionError::Storage(e)),
            }
        }) {
            Ok(()) => TransactionResult::Ok(()),
            Err(TransactionError::Storage(e)) => TransactionResult::Err(e),
            Err(TransactionError::Abort(())) => TransactionResult::Rollback,
        }
    }
}

#[derive(Clone)]
#[repr(transparent)]
pub struct Table(sled::Tree);

impl super::TableImpl for Table {
    type Iter = Iter;

    #[inline(always)]
    fn get(&self, key: impl AsRef<[u8]>) -> Result<Option<Buffer>> {
        self.0.get(key)
    }

    #[inline(always)]
    fn insert(&mut self, key: impl AsRef<[u8]>, value: impl Into<Buffer>) -> Result<()> {
        self.0.insert(key, value).map(|_| ())
    }

    fn remove(&mut self, key: impl AsRef<[u8]>) -> Result<()> {
        self.0.remove(key).map(|_| ())
    }

    #[inline(always)]
    fn first_kv(&self) -> Result<Option<(Buffer, Buffer)>> {
        self.0.first()
    }

    #[inline(always)]
    fn last_kv(&self) -> Result<Option<(Buffer, Buffer)>> {
        self.0.last()
    }

    #[inline(always)]
    fn prefix(&self, prefix: impl AsRef<[u8]>) -> Iter {
        Iter(self.0.scan_prefix(prefix))
    }
}

#[repr(transparent)]
pub struct Iter(sled::Iter);

impl Iterator for Iter {
    type Item = Result<(Buffer, Buffer)>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl DoubleEndedIterator for Iter {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back()
    }
}

impl super::IterImpl for Iter {}

pub struct Transaction<'a> {
    trees: &'a [TransactionalTree],
    names: &'a [Buffer],
}

impl<'a> Transaction<'a> {
    fn new(trees: &'a [TransactionalTree], names: &'a [Buffer]) -> Self {
        assert_eq!(trees.len(), names.len());
        Self { trees, names }
    }

    fn find_tree(&self, table: &Table) -> Result<&TransactionalTree> {
        self.trees
            .iter()
            .zip(self.names)
            .find_map(|(tree, name)| (table.0.name() == name).then_some(tree))
            .ok_or_else(|| {
                Error::Unsupported(
                    "cannot use trees from multiple databases in the same transaction".to_owned(),
                )
            })
    }

    fn transact<T>(
        &self,
        table: &Table,
        f: impl FnOnce(&TransactionalTree) -> std::result::Result<T, UnabortableTransactionError>,
    ) -> Result<T> {
        self.find_tree(table).and_then(|tree| {
            f(tree).map_err(|e| match e {
                UnabortableTransactionError::Conflict => {
                    unreachable!("We don't do concurrency")
                }
                UnabortableTransactionError::Storage(e) => e,
            })
        })
    }
}

impl super::TransactionImpl for Transaction<'_> {
    type Table = Table;

    fn get(&self, table: &Self::Table, key: impl Into<Buffer>) -> Result<Option<Buffer>> {
        self.transact(table, |tree| tree.get(key.into()))
    }

    fn insert(
        &mut self,
        table: &mut Self::Table,
        key: impl Into<Buffer>,
        value: impl Into<Buffer>,
    ) -> Result<()> {
        self.transact(table, |tree| tree.insert(key.into(), value).map(|_| ()))
    }

    fn remove(&mut self, table: &mut Self::Table, key: impl Into<Buffer>) -> Result<()> {
        self.transact(table, |tree| tree.remove(key.into()).map(|_| ()))
    }
}
