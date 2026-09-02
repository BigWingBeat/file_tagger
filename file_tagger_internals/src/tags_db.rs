use std::{
    cmp::Ordering,
    fmt::{Debug, Display, Formatter},
    path::{Path, PathBuf},
    str::Utf8Error,
    sync::{Arc, Mutex},
};

use estr::Estr;
use miette::IntoDiagnostic;
use scru64::{Scru64Generator, Scru64Id, generator::NodeSpec, id::RangeError};
use thiserror::Error;

use crate::{
    DB_FOLDER_NAME, Transaction,
    app_data::RecentFolder,
    database::{
        self, Buffer, Conflict, Database, DbApi, DeserError, DeserKvError, NoTransaction, Table,
    },
    serde::{
        AsBytes, Bytes, DerefProxy, FromBytes, Prefixable, Reader, SizeHint, SmallSortedSet,
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

    pub fn initialize_transaction(self) -> Result<ActiveTransactionDatabaseState, database::Error> {
        self.database
            .initialize_transaction()
            .map(|database| ActiveTransactionDatabaseState {
                database,
                folder: self.folder,
            })
    }

    pub fn tag_entry_by_name(&self, tag: &Tag) -> Result<Option<Entry>, DeserError<Entry>> {
        self.database.tag_entry_by_name(tag)
    }

    pub fn tag_exists(&self, tag: &Tag) -> Result<bool, DeserError<Entry>> {
        self.database.tag_exists(tag)
    }

    pub fn search_tags_names_by_prefix(
        &self,
        prefix: &str,
    ) -> impl Iterator<Item = Result<Tag, DeserKvError<Tag, Entry>>> {
        self.database.search_tags_names_by_prefix(prefix)
    }
}

#[derive(Clone)]
pub struct TagsDatabase<Transaction = NoTransaction> {
    database: Database<Transaction>,
    /// Lookup which entries tags are applied to
    ///
    /// Key: tag name
    /// Value: list of entry IDs (no duplicates)
    ///
    /// Note: This is a separate table from `entries_by_data` to enable prefix lookups on the tag name. That doesn't work with
    /// `entries_by_data` because of the tuple length prefix, as to know the correct length prefix you have to know the full tag
    entries_by_tag: Table<Tag, SmallSortedSet<Entry>>,
    /// Lookup which entries have tags with specific data
    ///
    /// Key: composite (tag name + tag data)
    /// Value: list of entry IDs (no duplicates)
    ///
    /// Note: data in key is split by word for strings ("inverted index"). Tags with binary data, and tags without any data, are
    /// not present in this table at all, as there is no way to search for specific data values for such tags
    entries_by_data: Table<(Tag, Buffer), SmallSortedSet<Entry>>,
    /// Lookup which tags are applied to entries
    ///
    /// Key: entry ID
    /// Value: list of tag names (no duplicates)
    tags_by_entry: Table<Entry, SmallSortedSet<Tag>>,
    /// Lookup values of specific tag instances on specific entries
    ///
    /// Key: composite (entry ID + tag name)
    /// Value: tag data
    ///
    /// Note: Tags without any associated data are not present, tag instances with empty data (e.g. empty strings) are present
    tag_values: Table<(Entry, Tag), Buffer>,
    /// Convert tag names to their associated tag entries (that is, the "meta" entry that describes that tag)
    ///
    /// Key: tag name
    /// Value: tag entry ID
    tag_entries: Table<Tag, Entry>,
    /// `Scru64Generator` already impls `Clone` itself but I'm not sure what the implications of cloning a generator are
    /// in terms of the generated IDs, so I'm making it a singleton type thing just in case
    generator: Arc<Mutex<Scru64Generator>>,
}

/// Transaction-agnostic methods
impl<T: DbApi> TagsDatabase<T> {
    fn open_tables(mut database: Database<T>) -> miette::Result<Self> {
        let entries_by_tag = database.open_table("EntriesByTag").into_diagnostic()?;
        let entries_by_data = database.open_table("EntriesByData").into_diagnostic()?;
        let tags_by_entry = database.open_table("TagsByEntry").into_diagnostic()?;
        let tag_values = database.open_table("TagValues").into_diagnostic()?;
        let tag_entries = database.open_table("TagEntries").into_diagnostic()?;
        let generator = Arc::new(Mutex::new(
            init_or_resume_generator(&database, &tags_by_entry).into_diagnostic()?,
        ));
        Ok(Self {
            database,
            entries_by_tag,
            entries_by_data,
            tags_by_entry,
            tag_values,
            tag_entries,
            generator,
        })
    }

    fn insert_tag_on_entry(&mut self, tag: Tag, entry: Entry) -> miette::Result<()> {
        // Update entries_by_tag: append entry to value (unless already present)
        // Update entries_by_data: append entry to value (unless already present)
        // Update tags_by_entry: append tag to value (unless already present)
        // Insert into tag_values

        let value = Buffer::default();

        self.database
            .fetch_update(&mut self.entries_by_tag, &tag, |entries| {
                let mut entries = entries.unwrap_or_default();
                let _ = entries.insert(entry);
                Some(entries)
            })
            .into_diagnostic()?;

        let key = (tag, value);
        self.database
            .fetch_update(&mut self.entries_by_data, &key, |entries| {
                let mut entries = entries.unwrap_or_default();
                let _ = entries.insert(entry);
                Some(entries)
            })
            .into_diagnostic()?;
        let (tag, value) = key;

        self.database
            .fetch_update(&mut self.tags_by_entry, &entry, |tags| {
                let mut tags = tags.unwrap_or_default();
                let _ = tags.insert(tag);
                Some(tags)
            })
            .into_diagnostic()?;

        let key = (entry, tag);
        self.database
            .insert(&mut self.tag_values, &key, &value)
            .into_diagnostic()
    }

    fn tag_entry_by_name(&self, tag: &Tag) -> Result<Option<Entry>, DeserError<Entry>> {
        self.database.get(&self.tag_entries, tag)
    }

    fn tag_exists(&self, tag: &Tag) -> Result<bool, DeserError<Entry>> {
        self.database
            .get(&self.tag_entries, tag)
            .map(|entry| entry.is_some())
    }

    fn search_tags_names_by_prefix(
        &self,
        prefix: &str,
    ) -> impl Iterator<Item = Result<Tag, DeserKvError<Tag, Entry>>> {
        self.database
            .prefix(&self.tag_entries, prefix)
            .map(|kv| kv.map(|(k, _)| k))
    }
}

/// Ditto
impl<T> TagsDatabase<T> {
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
}

/// No transaction methods
impl TagsDatabase {
    fn open(path: impl AsRef<Path>) -> miette::Result<Self> {
        Database::open(path)
            .into_diagnostic()
            .and_then(Self::open_tables)
    }

    fn open_temporary() -> miette::Result<Self> {
        Database::open_temporary()
            .into_diagnostic()
            .and_then(Self::open_tables)
    }

    fn initialize_transaction(self) -> Result<TagsDatabase<Transaction>, database::Error> {
        self.database
            .initialize_transaction()
            .map(|database| TagsDatabase {
                database,
                entries_by_tag: self.entries_by_tag,
                entries_by_data: self.entries_by_data,
                tags_by_entry: self.tags_by_entry,
                tag_values: self.tag_values,
                tag_entries: self.tag_entries,
                generator: self.generator,
            })
    }
}

/// Yes transaction methods
impl TagsDatabase<Transaction> {
    fn commit(self) -> (TagsDatabase, Result<Result<(), Conflict>, database::Error>) {
        let (database, result) = self.database.commit();
        (
            TagsDatabase {
                database,
                entries_by_tag: self.entries_by_tag,
                entries_by_data: self.entries_by_data,
                tags_by_entry: self.tags_by_entry,
                tag_values: self.tag_values,
                tag_entries: self.tag_entries,
                generator: self.generator,
            },
            result,
        )
    }

    fn rollback(self) -> TagsDatabase {
        TagsDatabase {
            database: self.database.rollback(),
            entries_by_tag: self.entries_by_tag,
            entries_by_data: self.entries_by_data,
            tags_by_entry: self.tags_by_entry,
            tag_values: self.tag_values,
            tag_entries: self.tag_entries,
            generator: self.generator,
        }
    }
}

pub struct ActiveTransactionDatabaseState {
    database: TagsDatabase<Transaction>,
    folder: RecentFolder,
}

impl ActiveTransactionDatabaseState {
    /// Does not mutate the database. If you want to persist the returned entry, you must write it to the database yourself.
    pub fn generate_entry(&mut self) -> Entry {
        self.database.generate_entry()
    }

    pub fn tag_exists(&self, tag: &Tag) -> Result<bool, DeserError<Entry>> {
        self.database.tag_exists(tag)
    }

    pub fn search_tags_names_by_prefix(
        &self,
        prefix: &str,
    ) -> impl Iterator<Item = Result<Tag, DeserKvError<Tag, Entry>>> {
        self.database.search_tags_names_by_prefix(prefix)
    }

    pub fn commit(self) -> (DatabaseState, Result<Result<(), Conflict>, database::Error>) {
        let (database, result) = self.database.commit();
        (
            DatabaseState {
                database,
                folder: self.folder,
            },
            result,
        )
    }

    pub fn rollback(self) -> DatabaseState {
        DatabaseState {
            database: self.database.rollback(),
            folder: self.folder,
        }
    }
}

/// Identifies a single entry in the database, which can have many associated tags.
/// Value is stored in big-endian form for correct lexicographic ordering.
/// Used for non-tag entries
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
#[repr(transparent)]
pub struct Tag(Estr);

impl PartialOrd for Tag {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Tag {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.as_str().cmp(other.0.as_str())
    }
}

impl SizeHint for Tag {
    const SIZE_HINT: Option<usize> = None;
}

impl FromBytes for Tag {
    type Error = Utf8Error;

    fn try_from(bytes: &mut Reader) -> Result<Self, Self::Error> {
        FromBytes::try_from(bytes).map(Self)
    }
}

impl AsBytes for Tag {
    type Bytes = String;

    fn as_bytes(&self) -> Bytes<'_, Self::Bytes> {
        self.0.as_bytes()
    }
}

impl<T: AsRef<str> + ?Sized> Prefixable<T> for Tag {
    type Prefix = String;

    fn prefix(prefix: &T) -> Self::Prefix {
        Estr::prefix(prefix)
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
        f.pad(self.as_str())
    }
}

fn init_or_resume_generator<T: DbApi, Value: FromBytes>(
    db: &Database<T>,
    table: &Table<Entry, Value>,
) -> Result<Scru64Generator, DeserKvError<Entry, Value>> {
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

    db.last_kv(table).map(|kv| {
        Scru64Generator::new(kv.map_or(DEFAULT_NODE_ID, |(latest_id, _)| {
            // This only errors if the second parameter has a bad value, which will never happen because it's a known-good literal
            NodeSpec::with_node_prev(latest_id.0, NODE_ID_SIZE).unwrap()
        }))
    })
}
