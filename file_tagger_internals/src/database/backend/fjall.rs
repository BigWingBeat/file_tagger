use std::path::Path;

use fjall::{CompressionType, KeyspaceCreateOptions};

pub type Result<T> = fjall::Result<T>;

pub type Buffer = fjall::Slice;

#[repr(transparent)]
pub struct Builder(fjall::DatabaseBuilder<fjall::Database>);

impl super::BuilderImpl for Builder {
    type Database = Database;

    #[inline(always)]
    fn new_with_path(path: impl AsRef<Path>) -> Self {
        Self(fjall::Database::builder(path))
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

#[repr(transparent)]
pub struct Database(fjall::Database);

impl super::DatabaseImpl for Database {
    type Table = Table;

    #[inline(always)]
    fn open_table(&self, name: &str) -> Result<Self::Table> {
        self.0
            .keyspace(name, KeyspaceCreateOptions::default)
            .map(Table)
    }
}

#[repr(transparent)]
pub struct Table(fjall::Keyspace);

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
        Iter(self.0.prefix(prefix))
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
