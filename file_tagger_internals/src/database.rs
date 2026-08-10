use std::{
    convert::Infallible, io::Read, marker::PhantomData, ops::Deref, path::Path, str::Utf8Error,
};

use byteview::ByteView;
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

pub type Result<T, E = backend::Error> = std::result::Result<T, E>;

/// Like [`Deref`], but with extra `AsRef<[u8]>` bounds built-in so we can just put
/// `T: HasBytes` everywhere instead of `T: AsRef<[u8]> + Deref<Target: AsRef<[u8]>>`
pub trait HasBytes: AsRef<[u8]> {
    type Target: AsRef<[u8]> + ?Sized;
    #[must_use]
    fn deref(&self) -> &Self::Target;
}

impl<B> HasBytes for B
where
    B: Deref + AsRef<[u8]>,
    B::Target: AsRef<[u8]>,
{
    type Target = B::Target;

    fn deref(&self) -> &Self::Target {
        self
    }
}

// Arrays don't impl `Deref` and they NEVER WILL, but the compiler still complains about potential future-incompatability of
// overlapping impls between this and the above blanket impl, because a `Deref` impl COULD be added to arrays (EVEN THOUGH IT WON'T)
// so we have to do Bullshit to work around that

pub trait ICantBelieveItsNotDeref {
    type Target: ?Sized;
    fn deref(&self) -> &Self::Target;
}

#[repr(transparent)]
pub struct DerefProxy<T>(T);

impl<T> From<T> for DerefProxy<T> {
    fn from(array: T) -> Self {
        Self(array)
    }
}

impl<T: ICantBelieveItsNotDeref> Deref for DerefProxy<T> {
    type Target = T::Target;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl<T, U> AsRef<U> for DerefProxy<T>
where
    T: AsRef<U>,
    U: ?Sized,
{
    fn as_ref(&self) -> &U {
        self.0.as_ref()
    }
}

impl<T, const N: usize> ICantBelieveItsNotDeref for [T; N] {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

/// A bit like [`Cow`], but defined by the owned form of the type, via [`Deref`], instead of by the borrowed form.
/// This is needed, instead of just using `Cow`, because `Cow` fixes the owned form to be exactly one type,
/// based on the borrowed form, via [`ToOwned`], but we need to be able to specify multiple owned forms that each have
/// the same borrowed form.
pub enum Bytes<'a, B>
where
    B: HasBytes,
{
    Borrowed(&'a B::Target),
    Owned(B),
}

impl<B> Bytes<'_, B>
where
    B: HasBytes,
{
    pub fn len(&self) -> usize {
        self.as_ref().len()
    }
}

impl<B: HasBytes> AsRef<[u8]> for Bytes<'_, B> {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Borrowed(b) => b.as_ref(),
            Self::Owned(b) => b.as_ref(),
        }
    }
}

/// Does this type have a constant size (in bytes) when serialized, or is it variable?
/// If `SIZE_HINT` is `Some(_)`, types should always consume exactly that many bytes from the input buffer in their `FromBytes`
/// impl. If it is `None`, types should instead consume the entire buffer.
pub trait SizeHint {
    const SIZE_HINT: Option<usize>;
}

/// A bit like `AsRef<[u8]>`, but allows returning owned data too, which enables both directly returning some bytes without
/// copying, as well as serializing data into arbitrary bytes and returning that
pub trait AsBytes: SizeHint {
    type Bytes: HasBytes;
    fn as_bytes(&self) -> Bytes<'_, Self::Bytes>;
}

/// A bit like `TryFrom<&[u8]>`, but controlled by us to workaround the orphan rule (we need to impl deser logic for foreign types).
/// Takes a `&mut &[u8]` to encourage using `std::io::read` methods, but without obfuscating that the source is just a `&[u8]`
/// (as would happen with `bytes: impl Read`), so that implementors can be confident that the read methods can't return weird errors.
pub trait FromBytes: SizeHint + Sized {
    type Error;
    fn try_from(bytes: &mut &[u8]) -> Result<Self, Self::Error>;
}

/// Kind of an extension trait to mirror std `from`/`into` ergonomics (not `TryInto` because orphan rule)
pub trait BytesInto<T> {
    type Error;
    fn bytes_into(self) -> Result<T, Self::Error>;
}

#[derive(Error)]
pub enum BytesIntoError<T: FromBytes> {
    #[error("had {0} bytes left over (out of {1} total bytes) after deserializing {ty}", ty = std::any::type_name::<T>())]
    ExpectedEof(usize, usize),
    #[error("error deserializing {ty}", ty = std::any::type_name::<T>())]
    // This can't be `#[from]` because `T` *could* be `Self` and cause a conflict with the blanket identity fn impl of `From`...
    Deser(#[source] T::Error),
}

/// So no implicit trait bound on `T: Debug` from `#[derive(Debug)]`
impl<T: FromBytes> std::fmt::Debug for BytesIntoError<T>
where
    T::Error: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExpectedEof(left_over, total) => f
                .debug_tuple("ExpectedEof")
                .field(left_over)
                .field(total)
                .finish(),
            Self::Deser(e) => f.debug_tuple("Deser").field(e).finish(),
        }
    }
}

impl<T: FromBytes> BytesInto<T> for &Buffer {
    type Error = BytesIntoError<T>;

    fn bytes_into(self) -> Result<T, Self::Error> {
        let total_bytes = self.len();
        let mut bytes = self.as_ref();
        T::try_from(&mut bytes)
            .map_err(BytesIntoError::Deser)
            .and_then(|result| {
                bytes
                    .is_empty()
                    .then_some(result)
                    .ok_or_else(|| BytesIntoError::ExpectedEof(bytes.len(), total_bytes))
            })
    }
}

/// Trait with bounds that enforce that the implementing type can be serialized and deserialized, which
/// makes it usable as a key or value in untyped KV databases, which only deal in `&[u8]`.
pub trait Serde: AsBytes + FromBytes {}

impl<T> Serde for T where T: AsBytes + FromBytes {}

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

/// Like a `Vec<String>` but where all the strings are stored inline in a single heap allocation.
pub struct InlineStrVec {
    // The format this uses is little-endian `u32` length prefixes for each string in the buffer
    buffer: backend::Buffer,
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
    #[error(
        "unexpected EOF while reading length prefix: expected the buffer to have at least {0} bytes but it had only {1} bytes total"
    )]
    LengthPrefixUnexpectedEOF(usize, usize),
    #[error(
        "unexpected EOF while reading string: expected the buffer to have at least {0} bytes but it had only {1} bytes total"
    )]
    StringUnexpectedEOF(usize, usize),
    #[error(transparent)]
    InvalidUTF8(#[from] Utf8Error),
}

impl SizeHint for InlineStrVec {
    const SIZE_HINT: Option<usize> = None;
}

impl FromBytes for InlineStrVec {
    type Error = InlineStrVecParseError;

    fn try_from(bytes: &mut &[u8]) -> Result<Self, Self::Error> {
        // It's important for this to check that the buffer is well-formed so the iterator impl can be infallible.

        // `bytes` is a mut ref to a shared ref to a buffer of bytes. In the while-loop, we mutate that inner shared ref
        // to point to smaller subslices, until it is empty due to pointing to a 0-length subslice (and we have verified
        // that the entire buffer is well-formed). `buffer` here is a copy of that inner shared ref, that points to the
        // entire buffer, separately from the shared ref in `bytes` that we are mutating.
        // We do it like this so that we can copy the whole buffer all at once at the end, rather than bit-by-bit in the loop,
        // and so that the caller is also able to observe us "consuming" the bytes in the buffer, from us mutating that shared ref.
        let buffer = *bytes;
        while !bytes.is_empty() {
            let bytes_read = buffer.len() - bytes.len();
            // The format is a little-endian u32, followed by a string of that many bytes, repeated to the end of the buffer
            let mut prefix = [0; _];
            bytes
                .read_exact(&mut prefix)
                // As we are reading from an in-memory `&[u8]`, the only possible IO error is `UnexpectedEof`
                .map_err(|_| {
                    InlineStrVecParseError::LengthPrefixUnexpectedEOF(
                        bytes_read + prefix.len(),
                        buffer.len(),
                    )
                })?;
            let len = u32::from_le_bytes(prefix) as _;
            let string = bytes.split_off(..len).ok_or_else(|| {
                InlineStrVecParseError::StringUnexpectedEOF(
                    bytes_read + prefix.len() + len,
                    buffer.len(),
                )
            })?;
            // We just need to check that the bytes in the buffer are valid UTF-8, doing anything with the str is unneeded
            let _ = str::from_utf8(string)?;
        }
        Ok(Self {
            buffer: buffer.into(),
        })
    }
}

impl AsBytes for InlineStrVec {
    type Bytes = backend::Buffer;

    fn as_bytes(&self) -> Bytes<'_, Self::Bytes> {
        Bytes::Borrowed(self.buffer.as_ref())
    }
}

impl<'a, A: AsRef<str> + ?Sized> FromIterator<&'a A> for InlineStrVec {
    fn from_iter<T: IntoIterator<Item = &'a A>>(iter: T) -> Self {
        // When the input is already valid strings, as opposed to raw bytes, construction is infallible
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
const INLINE_SIZE: usize = cfg_select! {
    feature = "backend_fjall" => 20,
    feature = "backend_sled" => 22,
};

pub type SmallVec<T> = smallvec::SmallVec<[T; INLINE_SIZE]>;

impl<T> SizeHint for SmallVec<T> {
    const SIZE_HINT: Option<usize> = None;
}

/// hack
pub trait StableSize {
    /// hack
    const SIZE: usize;
}

/// hack
impl<T: SizeHint> StableSize for T {
    /// hack
    const SIZE: usize = T::SIZE_HINT.unwrap();
}

/// Only supported for element types with constant serialized size
impl<T> AsBytes for SmallVec<T>
where
    T: AsBytes + StableSize,
{
    type Bytes = backend::Buffer;

    fn as_bytes(&'_ self) -> Bytes<'_, Self::Bytes> {
        let total_bytes = T::SIZE * self.len();
        let mut buffer = ByteView::builder(total_bytes);
        // We would use `as_chunks_mut` here but we can't because of bullshit
        for (element, chunk) in self.iter().zip(buffer.chunks_exact_mut(T::SIZE)) {
            let bytes = element.as_bytes();
            chunk.copy_from_slice(bytes.as_ref());
        }
        Bytes::Owned(buffer.freeze().into())
    }
}

/// Only supported for element types with constant serialized size
impl<T> FromBytes for SmallVec<T>
where
    T: FromBytes + StableSize,
{
    type Error = T::Error;

    fn try_from(bytes: &mut &[u8]) -> Result<Self, T::Error> {
        let elements = bytes.len() / T::SIZE;
        let mut vec = Self::with_capacity(elements);
        while !bytes.is_empty() {
            vec.push(T::try_from(bytes)?);
        }
        Ok(vec)
    }
}

// We only need 2-tuples for now

impl<T: SizeHint, U: SizeHint> SizeHint for (T, U) {
    // Unstable/non-const alternatives:
    // - `T::SIZE_HINT.zip_with(U::SIZE_HINT, Add::add)`
    // - `[T::SIZE_HINT, U::SIZE_HINT].into_iter().sum()`
    // - `T::SIZE_HINT.zip_with(U::SIZE_HINT, |t, u| t + u)`
    // - `T::SIZE_HINT.zip(U::SIZE_HINT).map(|(t, u)| t + u)`
    // - `T::SIZE_HINT.zip_with(U::SIZE_HINT, usize::strict_add)`
    const SIZE_HINT: Option<usize> = match (T::SIZE_HINT, U::SIZE_HINT) {
        (Some(t), Some(u)) => Some(t + u),
        _ => None,
    };
}

impl<T: AsBytes, U: AsBytes> AsBytes for (T, U) {
    type Bytes = backend::Buffer;

    fn as_bytes(&'_ self) -> Bytes<'_, Self::Bytes> {
        let (t, u) = self;
        let t = t.as_bytes();
        let u = u.as_bytes();
        if T::SIZE_HINT.is_none() {
            // If `T` has a variable size, it needs a length prefix (`U` doesn't because it just assumes the entire rest of the buffer)
            let prefix = (t.len() as u32).to_le_bytes();
            let len = prefix.len() + t.len() + u.len();
            let mut bytes = ByteView::builder(len);
            bytes[..prefix.len()].copy_from_slice(&prefix);
            bytes[prefix.len()..(prefix.len() + t.len())].copy_from_slice(t.as_ref());
            bytes[(prefix.len() + t.len())..].copy_from_slice(u.as_ref());
            Bytes::Owned(bytes.freeze().into())
        } else {
            // A built-in method that does exactly what we're manually doing above, but only for 2 input slices
            Bytes::Owned(ByteView::fused(t.as_ref(), u.as_ref()).into())
        }
    }
}

#[derive(Error)]
pub enum TupleError<T: FromBytes, U: FromBytes> {
    #[error(
        "unexpected EOF while reading length prefix: expected the buffer to have at least {0} bytes but it had only {1} bytes total"
    )]
    LengthPrefixUnexpectedEof(usize, usize),
    #[error(
        "unexpected EOF while deserializing first tuple element: expected the buffer to have at least {0} bytes but it had only {1} bytes total"
    )]
    FirstUnexpectedEof(usize, usize),
    // These can't be `#[from]` because ugh (see BytesIntoError *AND* DbError)
    #[error("error deserializing first tuple element")]
    First(#[source] T::Error),
    #[error("error deserializing second tuple element")]
    Second(#[source] U::Error),
}

impl<T: FromBytes, U: FromBytes> FromBytes for (T, U) {
    type Error = TupleError<T, U>;

    fn try_from(bytes: &mut &[u8]) -> Result<Self, Self::Error> {
        if T::SIZE_HINT.is_none() {
            let total = bytes.len();
            // If `T` has a variable size, we have a length prefix
            let mut prefix = [0; _];
            bytes
                .read_exact(&mut prefix)
                // As we are reading from an in-memory `&[u8]`, the only possible IO error is `UnexpectedEof`
                .map_err(|_| TupleError::LengthPrefixUnexpectedEof(prefix.len(), total))?;
            let len = u32::from_le_bytes(prefix) as _;
            // `split_off` is correct here, rather than `split_at_checked`, so that the bytes being consumed is visible to the caller
            let mut t = bytes
                .split_off(..len)
                .ok_or_else(|| TupleError::FirstUnexpectedEof(prefix.len() + len, total))?;
            let result = (
                T::try_from(&mut t).map_err(TupleError::First)?,
                // `U` doesn't have a length prefix as it is implicitly bounded by the end of the buffer
                U::try_from(bytes).map_err(TupleError::Second)?,
            );
            if !t.is_empty() {
                // The `split_off`, from the perspective of the caller, acts like eagerly consuming all of `T`'s bytes.
                // If `T` doesn't actually consume all of its bytes, we make that visible here
                *bytes = t;
            }
            Ok(result)
        } else {
            Ok((
                T::try_from(bytes).map_err(TupleError::First)?,
                U::try_from(bytes).map_err(TupleError::Second)?,
            ))
        }
    }
}

impl SizeHint for Buffer {
    const SIZE_HINT: Option<usize> = None;
}

impl AsBytes for Buffer {
    type Bytes = Self;

    fn as_bytes(&self) -> Bytes<'_, Self::Bytes> {
        Bytes::Borrowed(self.as_ref())
    }
}

impl FromBytes for Buffer {
    type Error = Infallible;

    fn try_from(bytes: &mut &[u8]) -> Result<Self, Self::Error> {
        // Use `take` to signal to the caller that we're "consuming" all of the bytes
        Ok(std::mem::take(bytes).into())
    }
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
