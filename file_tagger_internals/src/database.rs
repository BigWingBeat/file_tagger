use std::{marker::PhantomData, path::Path};

use thiserror::Error;

mod backend;
mod transaction;

pub use backend::{
    Buffer, Database, DatabaseImpl, Error, FinalizeTransaction, Iter as UntypedIter,
    Table as UntypedTable, Transaction, TransactionImpl, TransactionResult,
};
use backend::{Builder, BuilderImpl, TableImpl};
pub use transaction::{
    TransactionApi as UntypedTransactionApi,
    initialize_transaction as initialize_untyped_transaction,
};

use crate::serde::{BytesInto, BytesIntoError, FromBytes, Serde};

pub type Result<T, E = backend::Error> = std::result::Result<T, E>;

/// A strongly-typed wrapper around a single keyspace
pub struct Table<Key, Value> {
    table: backend::Table,
    marker: PhantomData<(Key, Value)>,
}

/// `#[derive(Clone)]` adds `where Key: Clone, Value: Clone` which is wrong as they are just `PhantomData` params
impl<Key, Value> Clone for Table<Key, Value> {
    fn clone(&self) -> Self {
        Self {
            table: self.table.clone(),
            marker: PhantomData,
        }
    }
}

#[derive(Error)]
pub enum DbError<Key: FromBytes, Value: FromBytes> {
    #[error(transparent)]
    Database(#[from] backend::Error),
    // These can't be `#[from]` because `Key` and `Value` *could* be the same types, resulting in two conflicting `From` impls...
    #[error("error deserializing database Key")]
    Key(#[source] BytesIntoError<Key>),
    #[error("error deserializing database Value")]
    Value(#[source] BytesIntoError<Value>),
}

/// Trait bounds...
impl<Key: FromBytes, Value: FromBytes> std::fmt::Debug for DbError<Key, Value>
where
    Key::Error: std::fmt::Debug,
    Value::Error: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(e) => f.debug_tuple("Database").field(e).finish(),
            Self::Key(key) => f.debug_tuple("Key").field(key).finish(),
            Self::Value(value) => f.debug_tuple("Value").field(value).finish(),
        }
    }
}

impl<Key: Serde, Value: Serde> Table<Key, Value> {
    pub fn open(database: &backend::Database, name: &str) -> backend::Result<Self> {
        database.open_table(name).map(|table| Self {
            table,
            marker: PhantomData,
        })
    }

    /// Retrieve a value from the `Table` if it exists.
    pub fn get(&self, key: &Key) -> Result<Option<Value>, DbError<Key, Value>> {
        Self::deser_result(self.table.get(key.as_bytes()))
    }

    /// Insert a key to a new value, overwriting any existing value.
    pub fn insert(&self, key: &Key, value: &Value) -> backend::Result<()> {
        self.table.insert(key.as_bytes(), value.as_bytes().as_ref())
    }

    /// Remove a key and its associated value from the table.
    pub fn remove(&self, key: &Key) -> backend::Result<()> {
        self.table.remove(key.as_bytes())
    }

    /// Retrieve the key that is lexicographically first in the table, and the associated value.
    pub fn first_kv(&self) -> Result<Option<(Key, Value)>, DbError<Key, Value>> {
        Self::deser_kv_result(self.table.first_kv())
    }

    /// Retrieve the key that is lexicographically last in the table, and the associated value.
    pub fn last_kv(&self) -> Result<Option<(Key, Value)>, DbError<Key, Value>> {
        Self::deser_kv_result(self.table.last_kv())
    }

    /// Returns an iterator over all entries in the table for which the value of the key starts with the given string of bytes.
    pub fn prefix(&self, prefix: impl AsRef<[u8]>) -> Iter<Key, Value> {
        Iter::new(self.table.prefix(prefix))
    }

    // These are technically not methods, as they have no `self` parameter,
    // but they are in the impl block anyway because they use the generic type parameters.

    fn deser_result(
        result: backend::Result<Option<backend::Buffer>>,
    ) -> Result<Option<Value>, DbError<Key, Value>> {
        match result {
            Ok(Some(value)) => value.bytes_into().map_err(DbError::Value).map(Some),
            Ok(None) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn deser_kv_result(
        result: backend::Result<Option<(backend::Buffer, backend::Buffer)>>,
    ) -> Result<Option<(Key, Value)>, DbError<Key, Value>> {
        match result {
            Ok(Some((key, value))) => {
                let key = key.bytes_into().map_err(DbError::Key)?;
                let value = value.bytes_into().map_err(DbError::Value)?;
                Ok(Some((key, value)))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

/// Typed wrapper around an untyped iterator
pub struct Iter<Key, Value> {
    iter: UntypedIter,
    marker: PhantomData<(Key, Value)>,
}

impl<Key, Value> Iter<Key, Value> {
    // This should not be public as we only want this to be constructed from typed tables with matching KV types
    fn new(iter: UntypedIter) -> Self {
        Self {
            iter,
            marker: PhantomData,
        }
    }
}

impl<Key: Serde, Value: Serde> Iterator for Iter<Key, Value> {
    type Item = Result<(Key, Value), DbError<Key, Value>>;

    fn next(&mut self) -> Option<Self::Item> {
        Table::<Key, Value>::deser_kv_result(self.iter.next().transpose()).transpose()
    }
}

impl<Key: Serde, Value: Serde> DoubleEndedIterator for Iter<Key, Value> {
    fn next_back(&mut self) -> Option<Self::Item> {
        Table::<Key, Value>::deser_kv_result(self.iter.next_back().transpose()).transpose()
    }
}

/// Typed wrapper around an untyped transaction
#[derive(Debug)]
pub struct TransactionApi(UntypedTransactionApi);

impl TransactionApi {
    pub fn get<Key: Serde, Value: Serde>(
        &self,
        table: &Table<Key, Value>,
        key: &Key,
    ) -> Result<Option<Value>, DbError<Key, Value>> {
        Table::<Key, Value>::deser_result(self.0.get(&table.table, key.as_bytes().as_ref()))
    }

    pub fn insert<Key: Serde, Value: Serde>(
        &mut self,
        table: &Table<Key, Value>,
        key: &Key,
        value: &Value,
    ) -> backend::Result<()> {
        self.0.insert(
            &table.table,
            key.as_bytes().as_ref(),
            value.as_bytes().as_ref(),
        )
    }

    pub fn remove<Key: Serde, Value: Serde>(
        &mut self,
        table: &Table<Key, Value>,
        key: &Key,
    ) -> backend::Result<()> {
        self.0.remove(&table.table, key.as_bytes().as_ref())
    }

    pub fn prefix<Key: Serde, Value: Serde>(
        &self,
        table: &Table<Key, Value>,
        prefix: impl Into<Buffer>,
    ) -> Iter<Key, Value> {
        Iter::new(self.0.prefix(&table.table, prefix))
    }

    pub fn commit(self) -> backend::Result<()> {
        self.0.commit()
    }

    pub fn rollback(self) {
        self.0.rollback()
    }
}

pub fn initialize_transaction(db: Database) -> TransactionApi {
    TransactionApi(initialize_untyped_transaction(db))
}

pub fn open(path: impl AsRef<Path>) -> backend::Result<Database> {
    Builder::new_with_path(path)
        .compression(true)
        .temporary(false)
        .open()
}

// Over-engineered nonsense to deal with potential file name collisions
// (This stuff isn't even used anymore anyway...)

pub fn open_temporary() -> backend::Result<Database> {
    let id = std::process::id() as u64;
    let mut range = id..;
    const MAX_ATTEMPTS: usize = 3;
    range
        .by_ref()
        .take(MAX_ATTEMPTS - 1)
        .map(try_open_temporary)
        .find(Result::is_ok)
        .unwrap_or_else(|| try_open_temporary(range.start))
}

fn try_open_temporary(id: u64) -> backend::Result<Database> {
    // I have no idea how this constant value is derived. It's from Bevy's `EntityHasher`
    const UPPER_PHI: u64 = 0x9e37_79b9_0000_0001;
    let hash = id.wrapping_mul(UPPER_PHI).rotate_left(32);
    let mut path = std::env::temp_dir();
    path.push(format!("{hash:x}"));
    Builder::new_with_path(path)
        .compression(false)
        .temporary(true)
        .open()
}
