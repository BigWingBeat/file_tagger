use std::{
    fmt::{Debug, Display, Formatter},
    path::{Path, PathBuf},
    str::Utf8Error,
    sync::{Arc, Mutex},
};

use byteview::StrView;
use miette::IntoDiagnostic;
use scru64::{Scru64Generator, Scru64Id, generator::NodeSpec, id::RangeError};
use thiserror::Error;

use crate::{
    DB_FOLDER_NAME, TransactionApi,
    app_data::RecentFolder,
    database::{Buffer, Database, DbError, Table},
    initialize_transaction,
    serde::{
        AsBytes, Bytes, DerefProxy, FromBytes, InlineStrVec, Reader, Serde, SizeHint, SmallVec,
        UnexpectedEof,
    },
};

#[derive(Clone)]
pub struct DatabaseState {
    database: TagsDatabase,
    folder: RecentFolder,
}

impl DatabaseState {
    pub fn active_folder(&self) -> &RecentFolder {
        &self.folder
    }

    pub fn create_temporary() -> miette::Result<Self> {
        TagsDatabase::open_temporary().map(|database| Self {
            database,
            folder: PathBuf::default().into(),
        })
    }

    pub fn open_in_folder(folder: RecentFolder) -> miette::Result<Self> {
        let mut db_folder = folder.clone();
        db_folder.path.push(DB_FOLDER_NAME);
        std::fs::create_dir(&db_folder.path)
            .or_else(|e| {
                // Ignore error if the dir already exists, as that's fine
                (e.kind() == std::io::ErrorKind::AlreadyExists)
                    .then_some(())
                    .ok_or(e)
            })
            .into_diagnostic()
            .and_then(|_| TagsDatabase::open(db_folder.path))
            .map(|database| Self { database, folder })
    }

    /// Does not mutate the database. If you want to persist the returned entry, you must write it to the database yourself.
    pub fn generate_entry(&mut self) -> Entry {
        self.database.generate_entry()
    }

    pub fn initialize_transaction(&self) -> TransactionApi {
        self.database.initialize_transaction()
    }

    pub fn tag_entry_by_name(&self, tag: &Tag) -> Result<Option<Entry>, DbError<Tag, Entry>> {
        self.database.tag_entry_by_name(tag)
    }

    pub fn tag_exists(&self, tag: &Tag) -> Result<bool, DbError<Tag, Entry>> {
        self.database.tag_exists(tag)
    }

    pub fn search_tags_names_by_prefix(
        &self,
        prefix: impl AsRef<[u8]>,
    ) -> impl Iterator<Item = Result<Tag, DbError<Tag, Entry>>> {
        self.database.search_tags_names_by_prefix(prefix)
    }
}

#[derive(Clone)]
pub struct TagsDatabase {
    database: Database,
    /// Lookup which entries tags are applied to
    ///
    /// Key: composite (tag name + tag data)
    /// Value: list of entry IDs
    ///
    /// Note: data in key is split by word for strings ("inverted index") and not present for binary blobs
    entries_by_tag: Table<(Tag, Buffer), SmallVec<Entry>>,
    /// Lookup which tags are applied to entries
    ///
    /// Key: entry ID
    /// Value: list of tag names
    tags_by_entry: Table<Entry, InlineStrVec>,
    /// Lookup values of specific tag instances on specific entries
    ///
    /// Key: composite (entry ID + tag name)
    /// Value: tag data
    ///
    /// Note: Tags without any associated data are not present, tag instances with empty data (e.g. empty strings) are present
    tag_values: Table<(Entry, Tag), Buffer>,
    /// Convert tag names to their associated tag entries
    ///
    /// Key: tag name
    /// Value: tag entry ID
    tag_entries: Table<Tag, Entry>,
    /// `Scru64Generator` already impls `Clone` itself but I'm not sure what the implications of cloning a generator are
    /// in terms of the generated IDs, so I'm making it a singleton type thing just in case
    generator: Arc<Mutex<Scru64Generator>>,
}

impl TagsDatabase {
    fn open_tables(database: Database) -> miette::Result<Self> {
        let entries_by_tag = Table::open(&database, "EntriesByTag").into_diagnostic()?;
        let tags_by_entry = Table::open(&database, "TagsByEntry").into_diagnostic()?;
        let tag_values = Table::open(&database, "TagValues").into_diagnostic()?;
        let tag_entries = Table::open(&database, "TagEntries").into_diagnostic()?;
        let generator = Arc::new(Mutex::new(
            init_or_resume_generator(&tags_by_entry).into_diagnostic()?,
        ));
        Ok(Self {
            database,
            entries_by_tag,
            tags_by_entry,
            tag_values,
            tag_entries,
            generator,
        })
    }

    fn open(path: impl AsRef<Path>) -> miette::Result<Self> {
        crate::database::open(path)
            .into_diagnostic()
            .and_then(Self::open_tables)
    }

    fn open_temporary() -> miette::Result<Self> {
        crate::database::open_temporary()
            .into_diagnostic()
            .and_then(Self::open_tables)
    }

    /// Does not mutate the database. If you want to persist the returned entry, you must write it to the database yourself.
    fn generate_entry(&mut self) -> Entry {
        // See: `scru64::new_sync()`
        const DELAY: std::time::Duration = std::time::Duration::from_millis(64);
        loop {
            if let Some(id) = self.generator.lock().unwrap().generate() {
                return id.into();
            } else {
                eprintln!("sleeping to generate entry ID");
                std::thread::sleep(DELAY);
            }
        }
    }

    fn tag_entry_by_name(&self, tag: &Tag) -> Result<Option<Entry>, DbError<Tag, Entry>> {
        self.tag_entries.get(tag)
    }

    fn tag_exists(&self, tag: &Tag) -> Result<bool, DbError<Tag, Entry>> {
        self.tag_entries.get(tag).map(|entry| entry.is_some())
    }

    fn search_tags_names_by_prefix(
        &self,
        prefix: impl AsRef<[u8]>,
    ) -> impl Iterator<Item = Result<Tag, DbError<Tag, Entry>>> {
        self.tag_entries.prefix(prefix).map(|kv| kv.map(|(k, _)| k))
    }

    pub fn initialize_transaction(&self) -> TransactionApi {
        initialize_transaction(self.database.clone())
    }
}

pub struct ActiveTransactionDatabaseState {
    db: DatabaseState,
    transaction: TransactionApi,
}

impl From<DatabaseState> for ActiveTransactionDatabaseState {
    fn from(db: DatabaseState) -> Self {
        let transaction = db.initialize_transaction();
        Self { db, transaction }
    }
}

impl ActiveTransactionDatabaseState {
    pub fn clone_db(&self) -> DatabaseState {
        self.db.clone()
    }

    /// Does not mutate the database. If you want to persist the returned entry, you must write it to the database yourself.
    pub fn generate_entry(&mut self) -> Entry {
        self.db.generate_entry()
    }

    pub fn tag_exists(&self, tag: &Tag) -> Result<bool, DbError<Tag, Entry>> {
        self.transaction
            .get(&self.db.database.tag_entries, tag)
            .map(|entry| entry.is_some())
    }

    pub fn search_tags_names_by_prefix(
        &self,
        prefix: impl Into<Buffer>,
    ) -> impl Iterator<Item = Result<Tag, DbError<Tag, Entry>>> {
        self.transaction
            .prefix(&self.db.database.tag_entries, prefix)
            .map(|kv| kv.map(|(k, _)| k))
    }

    pub fn commit(self) -> crate::database::Result<()> {
        self.transaction.commit()
    }

    pub fn rollback(self) {
        self.transaction.rollback()
    }
}

/// Identifies a single entry in the database, which can have many associated tags.
/// Value is stored in big-endian form for correct lexicographic ordering.
/// Used for non-tag entries
#[derive(Debug)]
#[repr(transparent)]
pub struct Entry(Scru64Id);

impl Display for Entry {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl SizeHint for Entry {
    const SIZE_HINT: Option<usize> = Some(size_of::<u64>());
}

impl AsBytes for Entry {
    type Bytes = DerefProxy<[u8; 8]>;

    fn as_bytes(&'_ self) -> Bytes<'_, Self::Bytes> {
        Bytes::Owned(self.0.to_u64().to_be_bytes().into())
    }
}

#[derive(Error, Debug)]
pub enum EntryParseError {
    #[error("unexpected EOF while reading entry ID")]
    UnexpectedEOF(#[from] UnexpectedEof),
    #[error(transparent)]
    Range(#[from] RangeError<u64>),
}

impl FromBytes for Entry {
    type Error = EntryParseError;

    fn try_from(bytes: &mut Reader) -> Result<Self, Self::Error> {
        bytes
            .read_exact()
            .map_err(Into::into)
            .map(u64::from_be_bytes)
            .and_then(|u64| u64.try_into().map_err(Into::into))
            .map(Self)
    }
}

impl From<Scru64Id> for Entry {
    fn from(value: Scru64Id) -> Self {
        Self(value)
    }
}

impl TryFrom<[u8; 8]> for Entry {
    type Error = RangeError<u64>;

    fn try_from(bytes: [u8; 8]) -> Result<Self, Self::Error> {
        u64::from_be_bytes(bytes).try_into().map(Self)
    }
}

/// User-facing UTF-8 identifier for a tag, which is a type that can have instances associated with specific entries.
/// Each tag also has its own associated entry, which can itself be tagged.
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Debug)]
#[repr(transparent)]
pub struct Tag(StrView);

impl SizeHint for Tag {
    const SIZE_HINT: Option<usize> = None;
}

impl FromBytes for Tag {
    type Error = Utf8Error;

    fn try_from(bytes: &mut Reader) -> Result<Self, Self::Error> {
        str::from_utf8(bytes.take_all()).map(Into::into).map(Self)
    }
}

impl AsBytes for Tag {
    type Bytes = String;

    fn as_bytes(&self) -> Bytes<'_, Self::Bytes> {
        Bytes::Borrowed(self.0.as_ref())
    }
}

impl From<&str> for Tag {
    fn from(string: &str) -> Self {
        Self(string.into())
    }
}

impl Tag {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for Tag {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

fn init_or_resume_generator<Value: Serde>(
    table: &Table<Entry, Value>,
) -> Result<Scru64Generator, DbError<Entry, Value>> {
    // Scru64 ids are always 64 bits. This parameter controls how many of those bits are allocated to a custom value we control.
    // This is useful for distributed systems with multiple nodes using the same ID space, but we are just an offline, local app,
    // so we don't need this. Instead, we set this to the smallest allowed number of bits (1), which maximizes the number of bits
    // allocated to the other (actually useful) parts of the ID.
    const NODE_ID_SIZE: u8 = 1;

    // This is infallible, but `unwrap` and similar methods aren't `const`, so we do this instead
    const DEFAULT_NODE_ID: NodeSpec = match NodeSpec::with_node_id(0, NODE_ID_SIZE) {
        Ok(id) => id,
        Err(_) => unreachable!(),
    };

    table.last_kv().map(|kv| {
        Scru64Generator::new(kv.map_or(DEFAULT_NODE_ID, |(latest_id, _)| {
            // This only errors if the second parameter has a bad value, which will never happen because it's a known-good literal
            NodeSpec::with_node_prev(latest_id.0, NODE_ID_SIZE).unwrap()
        }))
    })
}
