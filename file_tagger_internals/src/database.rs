use std::{io::Read, marker::PhantomData, path::Path, str::Utf8Error};

use miette::IntoDiagnostic;
use smallvec::SmallVec;
use thiserror::Error;

mod backend;

pub use backend::{
    Buffer, Database, DatabaseImpl, Error, Result, Table as UntypedTable, Transaction,
    TransactionImpl,
};
use backend::{Builder, BuilderImpl, TableImpl};

/// A bit like a [`Cow`], but defined by the owned form of the type, instead of by the borrowed form.
/// This is needed, instead of just using `Cow`, as we are fixing the borrowed form to be `&[u8]`, and letting the
/// owned form vary. `Cow` instead fixes the owned form to be exactly one type, based on the borrowed form, via [`ToOwned`]
pub enum Bytes<'a, B>
where
    B: AsRef<[u8]>,
{
    Borrowed(&'a B),
    Owned(B),
}

impl<B: AsRef<[u8]>> AsRef<[u8]> for Bytes<'_, B> {
    fn as_ref(&self) -> &[u8] {
        // These tro branches seemingly can't be folded into one, despite having identical bodies, for some reason...
        match self {
            Bytes::Borrowed(b) => b.as_ref(),
            Bytes::Owned(b) => b.as_ref(),
        }
    }
}

/// A bit like `AsRef<[u8]>`, but allows returning owned data too, which enables both directly returning some bytes without
/// copying, as well as serializing data into arbitrary bytes and returning that
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

/// Trait with bounds that enforce that the implementing type can be serialized and deserialized, which
/// makes it usable as a key or value in untyped KV databases, which only deal in `&[u8]`.
pub trait Serde<'a>: AsBytes + TryFrom<&'a [u8], Error = Self::E> {
    type E: std::error::Error + Send + Sync + 'static;
}

impl<'a, T> Serde<'a> for T
where
    T: AsBytes + TryFrom<&'a [u8]>,
    <T as TryFrom<&'a [u8]>>::Error: std::error::Error + Send + Sync + 'static,
{
    type E = <T as TryFrom<&'a [u8]>>::Error;
}

/// Kind of a "marker" trait for types intended to be used as database values
pub trait Value<'a>: Serde<'a> {}

impl<'a, T> Value<'a> for T where T: Serde<'a> {}

/// Kind of a "marker" trait for types intended to be used as database keys
pub trait Key<'a>: Serde<'a> {}

impl<'a, T> Key<'a> for T where T: Serde<'a> {}

/// A strongly-typed wrapper around a single keyspace
pub struct Table<Key, Value> {
    database: backend::Table,
    marker: PhantomData<(Key, Value)>,
}

/// `#[derive(Clone)]` adds `where Key: Clone, Value: Clone` which is wrong as they are just `PhantomData` params
impl<Key, Value> Clone for Table<Key, Value> {
    fn clone(&self) -> Self {
        Self {
            database: self.database.clone(),
            marker: PhantomData,
        }
    }
}

impl<Key, Value> Table<Key, Value>
where
    Key: for<'a> self::Key<'a>,
    Value: for<'a> self::Value<'a>,
{
    pub fn open(database: &backend::Database, name: &str) -> backend::Result<Self> {
        database.open_table(name).map(|database| Self {
            database,
            marker: PhantomData,
        })
    }

    /// Retrieve a value from the `Table` if it exists.
    pub fn get(&self, key: &Key) -> miette::Result<Option<Value>> {
        Self::parse_result(self.database.get(key.as_bytes()))
    }

    /// Insert a key to a new value, overwriting any existing value
    pub fn insert(&self, key: &Key, value: &Value) -> backend::Result<()> {
        self.database
            .insert(key.as_bytes(), value.as_bytes().as_ref())
    }

    pub fn first_kv(&self) -> miette::Result<Option<(Key, Value)>> {
        Self::parse_kv_result(self.database.first_kv())
    }

    pub fn last_kv(&self) -> miette::Result<Option<(Key, Value)>> {
        Self::parse_kv_result(self.database.last_kv())
    }

    pub fn prefix(&self, prefix: impl AsRef<[u8]>) -> backend::Iter {
        self.database.prefix(prefix)
    }

    fn parse_result(
        result: backend::Result<Option<backend::Buffer>>,
    ) -> miette::Result<Option<Value>> {
        match result {
            Ok(Some(value)) => value.as_ref().try_into().into_diagnostic().map(Some),
            Ok(None) => Ok(None),
            Err(e) => Err(e).into_diagnostic(),
        }
    }

    fn parse_kv_result(
        result: backend::Result<Option<(backend::Buffer, backend::Buffer)>>,
    ) -> miette::Result<Option<(Key, Value)>> {
        match result {
            Ok(Some((key, value))) => {
                let key = key.as_ref().try_into().into_diagnostic()?;
                let value = value.as_ref().try_into().into_diagnostic()?;
                Ok(Some((key, value)))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e).into_diagnostic(),
        }
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

    fn try_from(value: &[u8]) -> std::result::Result<Self, Self::Error> {
        // It's important for this to check that the buffer is well-formed so the iterator impl can be infallible
        let mut buffer = value;
        while !buffer.is_empty() {
            // The format is a little-endian u32, followed by a string of that many bytes, repeated to the end of the buffer
            let mut len = [0u8; 4];
            buffer
                .read_exact(&mut len)
                // As we are reading from an in-memory `&[u8]`, the only possible error is `UnexpectedEof`
                .map_err(|_| InlineStrVecParseError::LengthPrefixUnexpectedEOF)?;
            let len = u32::from_le_bytes(len);
            let (string, rem) = buffer
                .split_at_checked(len as usize)
                .ok_or(InlineStrVecParseError::StringUnexpectedEOF)?;
            // We just need to check that the bytes in the buffer are valid UTF-8, doing anything with the str is unneeded
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
        // When the input is already valid strings, instead of raw bytes, construction is infallible
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

/// Most tag names should be less than this number of bytes.
/// This is the same threshhold used by the backend's buffer type.
/// For fjall, this is [`fjall::Slice`].
/// For sled, this is [`sled::IVec`].
pub const INLINE_SIZE: usize = cfg_select! {
    feature = "backend_fjall" => 20,
    feature = "backend_sled" => 22,
};

/// A type-safe(?) wrapper for composing multiple types into a single composite key
pub struct CompositeKey<Prefix, Suffix> {
    composite_value: SmallVec<[u8; INLINE_SIZE]>,
    marker: PhantomData<(Prefix, Suffix)>,
}

impl<Prefix, Suffix> CompositeKey<Prefix, Suffix>
where
    Prefix: for<'a> Key<'a>,
    Suffix: for<'a> Key<'a>,
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

/// This type isn't really "supposed" to be deserializable, as we can't reliably extract the
/// constituent prefix and suffix values from the composite bytes, but it's fine as long as this type is opaque
impl<Prefix, Suffix> From<&[u8]> for CompositeKey<Prefix, Suffix> {
    fn from(value: &[u8]) -> Self {
        Self {
            composite_value: SmallVec::from_slice(value),
            marker: PhantomData,
        }
    }
}

pub fn open(path: impl AsRef<Path>) -> Result<Database> {
    Builder::new_with_path(path)
        .compression(true)
        .temporary(false)
        .open()
}

pub fn open_temporary() -> Result<Database> {
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

fn try_open_temporary(id: u64) -> Result<Database> {
    const UPPER_PHI: u64 = 0x9e37_79b9_0000_0001;
    let hash = id.wrapping_mul(UPPER_PHI).rotate_left(32);
    let mut path = std::env::temp_dir();
    path.push(format!("{hash:x}"));
    Builder::new_with_path(path)
        .compression(false)
        .temporary(true)
        .open()
}
