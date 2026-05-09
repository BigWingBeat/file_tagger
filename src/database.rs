use std::{
    io::Read,
    marker::PhantomData,
    path::{Path, PathBuf},
    str::Utf8Error,
};

use miette::IntoDiagnostic;
use scru64::{Scru64Generator, Scru64Id, generator::NodeSpec};
use smallvec::SmallVec;
use thiserror::Error;

pub mod backend;

use backend::{DatabaseImpl, TableImpl};

pub struct DatabaseState {
    database: Database,
}

impl DatabaseState {
    pub fn create_temporary() -> miette::Result<Self> {
        Database::open_temporary().map(|database| Self { database })
    }

    pub fn open_in_folder(&mut self, mut path: PathBuf) -> miette::Result<&mut Database> {
        path.push(concat!('.', env!("CARGO_BIN_NAME")));
        std::fs::create_dir(&path)
            .or_else(|e| {
                (e.kind() == std::io::ErrorKind::AlreadyExists)
                    .then_some(())
                    .ok_or(e)
            })
            .unwrap();
        Database::open(path).map(|db| {
            self.database = db;
            &mut self.database
        })
    }

    /// Does not mutate the database. If you want to persist the returned entry, you must write it to the database yourself.
    pub fn generate_entry(&mut self) -> Entry {
        self.database.generate_entry()
    }
}

pub struct Database {
    database: backend::Database,
    entries_by_tag: EntriesByTag,
    tags_by_entry: TagsByEntry,
    tag_values: TagValues,
    tag_entries: TagEntries,
    generator: Scru64Generator,
}

impl Database {
    fn open_tables(database: backend::Database) -> miette::Result<Self> {
        let entries_by_tag = EntriesByTag::open(&database).into_diagnostic()?;
        let tags_by_entry = TagsByEntry::open(&database).into_diagnostic()?;
        let tag_values = TagValues::open(&database).into_diagnostic()?;
        let tag_entries = TagEntries::open(&database).into_diagnostic()?;
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
        backend::open(path)
            .into_diagnostic()
            .and_then(Self::open_tables)
    }

    fn open_temporary() -> miette::Result<Self> {
        backend::open_temporary()
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

/// Most tag names should be less than this number of bytes
/// This is the same threshhold used by [`fjall::Slice`]
/// For comparison, `sled::IVec` uses a threshhold of 22 bytes
const INLINE_SIZE: usize = 20;

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

/// Like a `Vec<String>` but where all the strings are stored inline in a single heap allocation.
pub struct InlineStrVec {
    // The format this uses is little-endian `u32` length prefixes for each string in the buffer
    buffer: backend::Buffer,
}

impl AsRef<[u8]> for InlineStrVec {
    fn as_ref(&self) -> &[u8] {
        self.buffer.as_ref()
    }
}

impl InlineStrVec {
    pub fn empty() -> Self {
        Self {
            buffer: backend::Buffer::empty(),
        }
    }

    pub fn iter(&self) -> InlineStrVecIter<'_> {
        InlineStrVecIter {
            buffer: &self.buffer,
        }
    }
}

impl<'a> IntoIterator for &'a InlineStrVec {
    type Item = &'a str;

    type IntoIter = InlineStrVecIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[derive(Error, Debug)]
pub enum InlineStrVecParseError {
    #[error("failed to read length prefix due to unexpected EOF")]
    LengthPrefixUnexpectedEOF,
    #[error("failed to read string due to unexpected EOF")]
    StringUnexpectedEOF,
    #[error(transparent)]
    InvalidUTF8(#[from] Utf8Error),
}

impl TryFrom<&[u8]> for InlineStrVec {
    type Error = InlineStrVecParseError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        // It's important for this to check that the buffer is well-formed so the iterator impl can be infallible
        let mut buffer = value;
        while !buffer.is_empty() {
            let mut len = [0u8; 4];
            buffer
                .read_exact(&mut len)
                .map_err(|_| InlineStrVecParseError::LengthPrefixUnexpectedEOF)?;
            let len = u32::from_le_bytes(len);
            let (string, rem) = buffer
                .split_at_checked(len as usize)
                .ok_or(InlineStrVecParseError::StringUnexpectedEOF)?;
            let _ = str::from_utf8(string)?;
            buffer = rem;
        }
        Ok(Self {
            buffer: value.into(),
        })
    }
}

impl<'a, A: AsRef<str> + ?Sized> FromIterator<&'a A> for InlineStrVec {
    fn from_iter<T: IntoIterator<Item = &'a A>>(iter: T) -> Self {
        let buffer = iter
            .into_iter()
            .map(AsRef::as_ref)
            .flat_map(|s| (s.len() as u32).to_le_bytes().into_iter().chain(s.bytes()))
            .collect();
        Self { buffer }
    }
}

pub struct InlineStrVecIter<'a> {
    buffer: &'a [u8],
}

impl<'a> Iterator for InlineStrVecIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.buffer.is_empty() {
            return None;
        }

        // We use unwraps here because `InlineStrVec` ensures it is well-formed on construction, so it should be fine
        let mut len = [0u8; 4];
        self.buffer.read_exact(&mut len).unwrap();
        let len = u32::from_le_bytes(len);
        let (value, remaining) = self.buffer.split_at(len as usize);
        self.buffer = remaining;
        Some(str::from_utf8(value).unwrap())
    }
}

pub enum Bytes<'a, B>
where
    B: AsRef<[u8]>,
{
    Borrowed(&'a B),
    Owned(B),
}

impl<B: AsRef<[u8]>> AsRef<[u8]> for Bytes<'_, B> {
    fn as_ref(&self) -> &[u8] {
        match self {
            Bytes::Borrowed(b) => b.as_ref(),
            Bytes::Owned(b) => b.as_ref(),
        }
    }
}

pub trait AsBytes {
    type Bytes: AsRef<[u8]>;
    fn as_bytes(&'_ self) -> Bytes<'_, Self::Bytes>;
}

impl<T> AsBytes for T
where
    T: AsRef<[u8]>,
{
    type Bytes = Self;
    fn as_bytes(&'_ self) -> Bytes<'_, Self::Bytes> {
        Bytes::Borrowed(self)
    }
}

pub trait Key: AsBytes {}

impl<T> Key for T where T: AsBytes {}

pub trait Value<'a>: AsBytes + TryFrom<&'a [u8], Error = Self::E> {
    type E: std::error::Error + Send + Sync + 'static;
}

impl<'a, T> Value<'a> for T
where
    T: AsBytes + TryFrom<&'a [u8]>,
    <T as TryFrom<&'a [u8]>>::Error: std::error::Error + Send + Sync + 'static,
{
    type E = <T as TryFrom<&'a [u8]>>::Error;
}

struct CompositeKey<Prefix, Suffix> {
    composite_value: SmallVec<[u8; INLINE_SIZE]>,
    marker: PhantomData<(Prefix, Suffix)>,
}

impl<Prefix, Suffix> CompositeKey<Prefix, Suffix>
where
    Prefix: Key,
    Suffix: Key,
{
    fn new(prefix: Prefix, suffix: Suffix) -> Self {
        let mut composite_value = SmallVec::from_slice(prefix.as_bytes().as_ref());
        composite_value.extend_from_slice(suffix.as_bytes().as_ref());
        Self {
            composite_value,
            marker: PhantomData,
        }
    }
}

impl<Prefix, Suffix> AsRef<[u8]> for CompositeKey<Prefix, Suffix> {
    fn as_ref(&self) -> &[u8] {
        &self.composite_value
    }
}

/// A strongly-typed wrapper around a single keyspace
pub trait Table: Sized {
    type Key: Key + ?Sized;
    type Value: for<'a> Value<'a> + ?Sized;

    const TABLE_NAME: &str;

    fn open(database: &backend::Database) -> backend::Result<Self>;

    fn table(&self) -> &backend::Table;
    fn table_mut(&mut self) -> &mut backend::Table;

    /// Retrieve a value from the `Table` if it exists.
    fn get(&self, key: &Self::Key) -> miette::Result<Option<Self::Value>> {
        Self::parse_result(self.table().get(key.as_bytes()))
    }

    /// Insert a key to a new value, overwriting any existing value
    fn insert(&self, key: &Self::Key, value: &Self::Value) -> backend::Result<()> {
        self.table()
            .insert(key.as_bytes(), value.as_bytes().as_ref())
    }

    fn parse_result(
        result: backend::Result<Option<backend::Buffer>>,
    ) -> miette::Result<Option<Self::Value>> {
        match result {
            Ok(Some(value)) => value.as_ref().try_into().map(Some).into_diagnostic(),
            Ok(None) => Ok(None),
            Err(e) => Err(e).into_diagnostic(),
        }
    }
}

fn init_or_resume_generator(table: &impl Table<Key = Entry>) -> miette::Result<Scru64Generator> {
    const DEFAULT_NODE_ID: NodeSpec = match NodeSpec::with_node_id(1, 1) {
        Ok(id) => id,
        Err(_) => unreachable!(),
    };

    table.table().last_kv().into_diagnostic().and_then(|kv| {
        kv.map_or(Ok(DEFAULT_NODE_ID), |(key, _)| {
            Entry::try_from(key.as_ref())
                .map(|latest_id| NodeSpec::with_node_prev(latest_id.0, 1).unwrap())
                .into_diagnostic()
        })
        .map(Scru64Generator::new)
    })
}

macro_rules! typed_table {
    ( $(#[$m:meta])* $p:vis struct $n:ident<$k:ty, $v:ty>; ) => {
        $(#[$m])*
        $p struct $n (crate::database::backend::Table);

        impl crate::database::Table for $n {
            type Key = $k;
            type Value = $v;

            const TABLE_NAME: &str = stringify!($n);

            fn open(database: &crate::database::backend::Database) -> crate::database::backend::Result<Self> {
                database.open_table(Self::TABLE_NAME).map(Self)
            }

            #[inline(always)]
            fn table(&self) -> &crate::database::backend::Table {
                &self.0
            }

            #[inline(always)]
            fn table_mut(&mut self) -> &mut crate::database::backend::Table {
                &mut self.0
            }
        }
    };
}

pub(crate) use typed_table;

typed_table! {
    /// Lookup which entries tags are applied to
    ///
    /// Key: composite (tag name + tag data)
    /// Value: list of entry IDs
    ///
    /// Note: data in key is split by word for strings ("inverted index") and not present for binary blobs
    struct EntriesByTag<CompositeKey<Tag, backend::Buffer>, Entry>;
}

typed_table! {
    /// Lookup which tags are applied to entries
    ///
    /// Key: entry ID
    /// Value: list of tag names
    struct TagsByEntry<Entry, InlineStrVec>;
}

typed_table! {
    /// Lookup values of specific tag instances on specific entries
    ///
    /// Key: composite (entry ID + tag name)
    /// Value: tag data
    struct TagValues<CompositeKey<Entry, Tag>, backend::Buffer>;
}

typed_table! {
    /// Convert tag names to their associated tag entries
    ///
    /// Key: tag name
    /// Value: tag entry ID
    struct TagEntries<Tag, Entry>;
}
