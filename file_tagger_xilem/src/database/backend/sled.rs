use std::path::Path;

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

#[repr(transparent)]
pub struct Database(sled::Db);

impl super::DatabaseImpl for Database {
    type Table = Table;

    #[inline(always)]
    fn open_table(&self, name: &str) -> Result<Self::Table> {
        self.0.open_tree(name).map(Table)
    }
}

#[repr(transparent)]
pub struct Table(sled::Tree);

impl super::TableImpl for Table {
    #[inline(always)]
    fn get(&self, key: impl AsRef<[u8]>) -> Result<Option<Buffer>> {
        self.0.get(key)
    }

    #[inline(always)]
    fn insert(&self, key: impl AsRef<[u8]>, value: impl Into<Buffer>) -> Result<()> {
        self.0.insert(key, value).map(|_| ())
    }

    #[inline(always)]
    fn first_kv(&self) -> Result<Option<(Buffer, Buffer)>> {
        self.0.first()
    }

    #[inline(always)]
    fn last_kv(&self) -> Result<Option<(Buffer, Buffer)>> {
        self.0.last()
    }
}
