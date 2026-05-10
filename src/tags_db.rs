use std::{
    path::{Path, PathBuf},
    str::Utf8Error,
};

use miette::IntoDiagnostic;
use scru64::{Scru64Generator, Scru64Id, generator::NodeSpec};
use smallvec::SmallVec;
use thiserror::Error;

use crate::database::{
    self, AsBytes, Buffer, Bytes, CompositeKey, Database, INLINE_SIZE, InlineStrVec, Table,
};

pub struct DatabaseState {
    database: TagsDatabase,
}

impl DatabaseState {
    pub fn create_temporary() -> miette::Result<Self> {
        TagsDatabase::open_temporary().map(|database| Self { database })
    }

    pub fn open_in_folder(&mut self, mut path: PathBuf) -> miette::Result<&mut TagsDatabase> {
        path.push(concat!('.', env!("CARGO_BIN_NAME")));
        std::fs::create_dir(&path)
            .or_else(|e| {
                (e.kind() == std::io::ErrorKind::AlreadyExists)
                    .then_some(())
                    .ok_or(e)
            })
            .unwrap();
        TagsDatabase::open(path).map(|db| {
            self.database = db;
            &mut self.database
        })
    }

    /// Does not mutate the database. If you want to persist the returned entry, you must write it to the database yourself.
    pub fn generate_entry(&mut self) -> Entry {
        self.database.generate_entry()
    }
}

pub struct TagsDatabase {
    database: Database,
    /// Lookup which entries tags are applied to
    ///
    /// Key: composite (tag name + tag data)
    /// Value: list of entry IDs
    ///
    /// Note: data in key is split by word for strings ("inverted index") and not present for binary blobs
    entries_by_tag: Table<CompositeKey<Tag, Buffer>, Entry>,
    /// Lookup which tags are applied to entries
    ///
    /// Key: entry ID
    /// Value: list of tag names
    tags_by_entry: Table<Entry, InlineStrVec>,
    /// Lookup values of specific tag instances on specific entries
    ///
    /// Key: composite (entry ID + tag name)
    /// Value: tag data
    tag_values: Table<CompositeKey<Entry, Tag>, Buffer>,
    /// Convert tag names to their associated tag entries
    ///
    /// Key: tag name
    /// Value: tag entry ID
    tag_entries: Table<Tag, Entry>,
    generator: Scru64Generator,
}

impl TagsDatabase {
    fn open_tables(database: Database) -> miette::Result<Self> {
        let entries_by_tag = Table::open(&database, "EntriesByTag").into_diagnostic()?;
        let tags_by_entry = Table::open(&database, "TagsByEntry").into_diagnostic()?;
        let tag_values = Table::open(&database, "TagValues").into_diagnostic()?;
        let tag_entries = Table::open(&database, "TagEntries").into_diagnostic()?;
        let generator = init_or_resume_generator(&tags_by_entry)?;
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
        database::open(path)
            .into_diagnostic()
            .and_then(Self::open_tables)
    }

    fn open_temporary() -> miette::Result<Self> {
        database::open_temporary()
            .into_diagnostic()
            .and_then(Self::open_tables)
    }

    /// Does not mutate the database. If you want to persist the returned entry, you must write it to the database yourself.
    fn generate_entry(&mut self) -> Entry {
        // See: `scru64::new_sync()`
        const DELAY: std::time::Duration = std::time::Duration::from_millis(64);
        loop {
            if let Some(id) = self.generator.generate() {
                return id.into();
            } else {
                eprintln!("sleeping to generate entry ID");
                std::thread::sleep(DELAY);
            }
        }
    }
}

/// Identifies a single entry in the database, which can have many associated tags.
/// Value is stored in big-endian form for correct lexicographic ordering.
/// Used for non-tag entries
#[repr(transparent)]
pub struct Entry(Scru64Id);

impl AsBytes for Entry {
    type Bytes = [u8; 8];
    fn as_bytes(&'_ self) -> Bytes<'_, Self::Bytes> {
        Bytes::Owned(self.0.to_u64().to_be_bytes())
    }
}

impl From<Scru64Id> for Entry {
    fn from(value: Scru64Id) -> Self {
        Self(value)
    }
}

#[derive(Error, Debug)]
pub enum EntryParseError {
    #[error("entry ID doesn't have enough bytes: {0} is less than 8")]
    NotEnoughBytes(usize),
    #[error("entry ID has too many bytes: {0} is more than 8")]
    TooManyBytes(usize),
    #[error(transparent)]
    Range(#[from] scru64::id::RangeError<u64>),
}

impl TryFrom<&[u8]> for Entry {
    type Error = EntryParseError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        match value.as_chunks() {
            (&[value], []) => u64::from_be_bytes(value)
                .try_into()
                .map(Self)
                .map_err(Into::into),
            ([], _) => Err(EntryParseError::NotEnoughBytes(value.len())),
            _ => Err(EntryParseError::TooManyBytes(value.len())),
        }
    }
}

/// User-facing identifier for a tag, which is a type that can have instances associated with specific entries.
/// Each tag also has its own associated entry, which can itself be tagged.
pub struct Tag(SmallVec<[u8; INLINE_SIZE]>);

impl AsRef<[u8]> for Tag {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl TryFrom<&[u8]> for Tag {
    type Error = Utf8Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        str::from_utf8(value).map(|value| Self(SmallVec::from_slice(value.as_bytes())))
    }
}

fn init_or_resume_generator<Value: for<'a> database::Value<'a>>(
    table: &Table<Entry, Value>,
) -> miette::Result<Scru64Generator> {
    // This is infallible, but `unwrap` and similar methods aren't `const`, so we do this instead
    const DEFAULT_NODE_ID: NodeSpec = match NodeSpec::with_node_id(1, 1) {
        Ok(id) => id,
        Err(_) => unreachable!(),
    };

    table.last_kv().map(|kv| {
        Scru64Generator::new(kv.map_or(DEFAULT_NODE_ID, |(latest_id, _)| {
            // This only errors if the second parameter has a bad value, which will never happen because it's a known-good literal
            NodeSpec::with_node_prev(latest_id.0, 1).unwrap()
        }))
    })
}
