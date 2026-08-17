use std::{marker::PhantomData, path::Path};

use thiserror::Error;

mod backend;
mod transaction;

pub use backend::{Buffer, Error, FinalizeTransaction, Iter as UntypedIter, Table as UntypedTable};
pub use transaction::Transaction;

use crate::serde::{AsBytes, BytesInto, BytesIntoError, FromBytes, Prefixable, Serde};
use backend::{Builder, BuilderImpl, DatabaseImpl, TableImpl};
use transaction::initialize_transaction;

pub type Result<T, E = backend::Error> = std::result::Result<T, E>;

#[derive(Error)]
pub enum DeserError<Value: FromBytes> {
    #[error(transparent)]
    Database(#[from] backend::Error),
    #[error(transparent)]
    Deser(#[from] BytesIntoError<Value>),
}

/// Trait bounds
impl<Value: FromBytes> std::fmt::Debug for DeserError<Value>
where
    Value::Error: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(e) => f.debug_tuple("Database").field(e).finish(),
            Self::Deser(value) => f.debug_tuple("Deser").field(value).finish(),
        }
    }
}

#[derive(Error)]
pub enum DeserKvError<Key: FromBytes, Value: FromBytes> {
    #[error(transparent)]
    Database(#[from] backend::Error),
    // These can't be `#[from]` because `Key` and `Value` *could* be the same types, resulting in two conflicting `From` impls...
    #[error("error deserializing database Key")]
    Key(#[source] BytesIntoError<Key>),
    #[error("error deserializing database Value")]
    Value(#[source] BytesIntoError<Value>),
}

/// Trait bounds...
impl<Key: FromBytes, Value: FromBytes> std::fmt::Debug for DeserKvError<Key, Value>
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

/// API for strongly-typed database operations
pub trait DbApi {
    /// Retrieve a value from the `Table` if it exists.
    fn get<Key: AsBytes, Value: FromBytes>(
        &self,
        table: &Table<Key, Value>,
        key: &Key,
    ) -> Result<Option<Value>, DeserError<Value>>;

    /// Insert a key to a new value, overwriting any existing value.
    fn insert<Key: AsBytes, Value: AsBytes>(
        &mut self,
        table: &mut Table<Key, Value>,
        key: &Key,
        value: &Value,
    ) -> backend::Result<()>;

    /// Remove a key and its associated value from the table.
    fn remove<Key: AsBytes, Value>(
        &mut self,
        table: &mut Table<Key, Value>,
        key: &Key,
    ) -> backend::Result<()>;

    /// Retrieve the key that is lexicographically first in the table, and the associated value.
    fn first_kv<Key: FromBytes, Value: FromBytes>(
        &self,
        table: &Table<Key, Value>,
    ) -> Result<Option<(Key, Value)>, DeserKvError<Key, Value>>;

    /// Retrieve the key that is lexicographically last in the table, and the associated value.
    fn last_kv<Key: FromBytes, Value: FromBytes>(
        &self,
        table: &Table<Key, Value>,
    ) -> Result<Option<(Key, Value)>, DeserKvError<Key, Value>>;

    /// Returns an iterator over all entries in the table for which the value of the key starts with the given string of bytes.
    fn prefix<Prefix, Key: Prefixable<Prefix> + FromBytes, Value: FromBytes>(
        &self,
        table: &Table<Key, Value>,
        prefix: &Prefix,
    ) -> Iter<Key, Value>;
}

/// This type should not be returned from or constructable outside this module,
/// to prevent bypassing an active transaction
#[derive(Clone)]
pub struct NoTransaction(());

/// Non-transactional
impl DbApi for NoTransaction {
    fn get<Key: AsBytes, Value: FromBytes>(
        &self,
        table: &Table<Key, Value>,
        key: &Key,
    ) -> Result<Option<Value>, DeserError<Value>> {
        deser_result(table.0.get(key.as_bytes()))
    }

    fn insert<Key: AsBytes, Value: AsBytes>(
        &mut self,
        table: &mut Table<Key, Value>,
        key: &Key,
        value: &Value,
    ) -> backend::Result<()> {
        table.0.insert(key.as_bytes(), value.as_bytes().as_ref())
    }

    fn remove<Key: AsBytes, Value>(
        &mut self,
        table: &mut Table<Key, Value>,
        key: &Key,
    ) -> backend::Result<()> {
        table.0.remove(key.as_bytes())
    }

    fn first_kv<Key: FromBytes, Value: FromBytes>(
        &self,
        table: &Table<Key, Value>,
    ) -> Result<Option<(Key, Value)>, DeserKvError<Key, Value>> {
        deser_kv_result(table.0.first_kv())
    }

    fn last_kv<Key: FromBytes, Value: FromBytes>(
        &self,
        table: &Table<Key, Value>,
    ) -> Result<Option<(Key, Value)>, DeserKvError<Key, Value>> {
        deser_kv_result(table.0.last_kv())
    }

    fn prefix<Prefix, Key: Prefixable<Prefix> + FromBytes, Value: FromBytes>(
        &self,
        table: &Table<Key, Value>,
        prefix: &Prefix,
    ) -> Iter<Key, Value> {
        Iter::new(table.0.prefix(Key::prefix(prefix)))
    }
}

impl DbApi for Transaction {
    fn get<Key: AsBytes, Value: FromBytes>(
        &self,
        table: &Table<Key, Value>,
        key: &Key,
    ) -> Result<Option<Value>, DeserError<Value>> {
        deser_result(self.get(&table.0, key.as_bytes().as_ref()))
    }

    fn insert<Key: AsBytes, Value: AsBytes>(
        &mut self,
        table: &mut Table<Key, Value>,
        key: &Key,
        value: &Value,
    ) -> backend::Result<()> {
        self.insert(
            &mut table.0,
            key.as_bytes().as_ref(),
            value.as_bytes().as_ref(),
        )
    }

    fn remove<Key: AsBytes, Value>(
        &mut self,
        table: &mut Table<Key, Value>,
        key: &Key,
    ) -> backend::Result<()> {
        self.remove(&mut table.0, key.as_bytes().as_ref())
    }

    fn first_kv<Key: FromBytes, Value: FromBytes>(
        &self,
        table: &Table<Key, Value>,
    ) -> Result<Option<(Key, Value)>, DeserKvError<Key, Value>> {
        deser_kv_result(self.first_kv(&table.0))
    }

    fn last_kv<Key: FromBytes, Value: FromBytes>(
        &self,
        table: &Table<Key, Value>,
    ) -> Result<Option<(Key, Value)>, DeserKvError<Key, Value>> {
        deser_kv_result(self.last_kv(&table.0))
    }

    fn prefix<Prefix, Key: Prefixable<Prefix> + FromBytes, Value: FromBytes>(
        &self,
        table: &Table<Key, Value>,
        prefix: &Prefix,
    ) -> Iter<Key, Value> {
        Iter::new(self.prefix(&table.0, Key::prefix(prefix).as_ref()))
    }
}

/// A transaction-aware wrapper around a database
#[derive(Clone)]
pub struct Database<Transaction = NoTransaction> {
    db: backend::Database,
    transaction: Transaction,
}

/// Transaction-agnostic methods
impl<T> Database<T> {
    pub fn open_table<Key, Value>(&mut self, name: &str) -> backend::Result<Table<Key, Value>> {
        self.db.open_table(name).map(Table::new)
    }
}

impl<T: DbApi> DbApi for Database<T> {
    fn get<Key: AsBytes, Value: FromBytes>(
        &self,
        table: &Table<Key, Value>,
        key: &Key,
    ) -> Result<Option<Value>, DeserError<Value>> {
        self.transaction.get(table, key)
    }

    fn insert<Key: AsBytes, Value: AsBytes>(
        &mut self,
        table: &mut Table<Key, Value>,
        key: &Key,
        value: &Value,
    ) -> backend::Result<()> {
        self.transaction.insert(table, key, value)
    }

    fn remove<Key: AsBytes, Value>(
        &mut self,
        table: &mut Table<Key, Value>,
        key: &Key,
    ) -> backend::Result<()> {
        self.transaction.remove(table, key)
    }

    fn first_kv<Key: FromBytes, Value: FromBytes>(
        &self,
        table: &Table<Key, Value>,
    ) -> Result<Option<(Key, Value)>, DeserKvError<Key, Value>> {
        self.transaction.first_kv(table)
    }

    fn last_kv<Key: FromBytes, Value: FromBytes>(
        &self,
        table: &Table<Key, Value>,
    ) -> Result<Option<(Key, Value)>, DeserKvError<Key, Value>> {
        self.transaction.last_kv(table)
    }

    fn prefix<Prefix, Key: Prefixable<Prefix> + FromBytes, Value: FromBytes>(
        &self,
        table: &Table<Key, Value>,
        prefix: &Prefix,
    ) -> Iter<Key, Value> {
        self.transaction.prefix(table, prefix)
    }
}

/// No transaction
impl Database {
    pub fn new(db: backend::Database) -> Self {
        Self {
            db,
            transaction: NoTransaction(()),
        }
    }

    pub fn initialize_transaction(self) -> Database<Transaction> {
        Database {
            db: self.db.clone(),
            transaction: initialize_transaction(self.db),
        }
    }

    pub fn open(path: impl AsRef<Path>) -> backend::Result<Self> {
        Builder::new_with_path(path)
            .compression(true)
            .temporary(false)
            .open()
            .map(Self::new)
    }

    // Over-engineered nonsense to deal with potential file name collisions
    // (This stuff isn't even used anymore anyway...)

    pub fn open_temporary() -> backend::Result<Self> {
        let id = std::process::id() as u64;
        let mut range = id..;
        const MAX_ATTEMPTS: usize = 3;
        range
            .by_ref()
            .take(MAX_ATTEMPTS - 1)
            .map(Self::try_open_temporary)
            .find(Result::is_ok)
            .unwrap_or_else(|| Self::try_open_temporary(range.start))
    }

    fn try_open_temporary(id: u64) -> backend::Result<Self> {
        // I have no idea how this constant value is derived. It's from Bevy's `EntityHasher`
        const UPPER_PHI: u64 = 0x9e37_79b9_0000_0001;
        let hash = id.wrapping_mul(UPPER_PHI).rotate_left(32);
        let mut path = std::env::temp_dir();
        path.push(format!("{hash:x}"));
        Builder::new_with_path(path)
            .compression(false)
            .temporary(true)
            .open()
            .map(Self::new)
    }
}

impl From<backend::Database> for Database {
    fn from(db: backend::Database) -> Self {
        Self::new(db)
    }
}

/// Yes transaction
impl Database<Transaction> {
    pub fn commit(self) -> (Database, backend::Result<()>) {
        (self.db.into(), self.transaction.commit())
    }

    pub fn rollback(self) -> Database {
        self.transaction.rollback();
        self.db.into()
    }
}

/// A strongly-typed wrapper around a single keyspace
#[repr(transparent)]
pub struct Table<Key, Value>(backend::Table, PhantomData<(Key, Value)>);

impl<Key, Value> Table<Key, Value> {
    // This should not be public so typed tables can't be mixed-and-matched
    fn new(table: backend::Table) -> Self {
        Self(table, PhantomData)
    }
}

/// `#[derive(Clone)]` adds `where Key: Clone, Value: Clone` which is wrong as they are just `PhantomData` params
impl<Key, Value> Clone for Table<Key, Value> {
    fn clone(&self) -> Self {
        Self(self.0.clone(), PhantomData)
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

impl<Key: FromBytes, Value: FromBytes> Iterator for Iter<Key, Value> {
    type Item = Result<(Key, Value), DeserKvError<Key, Value>>;

    fn next(&mut self) -> Option<Self::Item> {
        deser_kv_result(self.iter.next().transpose()).transpose()
    }
}

impl<Key: FromBytes, Value: FromBytes> DoubleEndedIterator for Iter<Key, Value> {
    fn next_back(&mut self) -> Option<Self::Item> {
        deser_kv_result(self.iter.next_back().transpose()).transpose()
    }
}

fn deser_result<Value: FromBytes>(
    result: backend::Result<Option<backend::Buffer>>,
) -> Result<Option<Value>, DeserError<Value>> {
    match result {
        Ok(Some(value)) => value.bytes_into().map_err(Into::into).map(Some),
        Ok(None) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn deser_kv_result<Key: FromBytes, Value: FromBytes>(
    result: backend::Result<Option<(backend::Buffer, backend::Buffer)>>,
) -> Result<Option<(Key, Value)>, DeserKvError<Key, Value>> {
    match result {
        Ok(Some((key, value))) => {
            let key = key.bytes_into().map_err(DeserKvError::Key)?;
            let value = value.bytes_into().map_err(DeserKvError::Value)?;
            Ok(Some((key, value)))
        }
        Ok(None) => Ok(None),
        Err(e) => Err(e.into()),
    }
}
