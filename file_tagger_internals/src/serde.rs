use std::{convert::Infallible, ops::Deref, str::Utf8Error};

use byteview::ByteView;
use thiserror::Error;

use crate::Buffer;

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
    #[must_use]
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

/// Wrapper around a byte slice to ensure that implementors of `FromBytes` always visibly "consume" the bytes they read,
/// rather than just invisibly copying them. Preferable to `std::io::Read` because we know that the byte source is just an
/// in-memory `&[u8]`, so we know we don't have to deal with any possible I/O errors
pub struct Reader<'a> {
    initial_total_bytes: usize,
    bytes: &'a [u8],
}

#[derive(Debug, Error)]
#[error("expected the buffer to have at least {0} bytes but it had only {1} bytes total")]
pub struct UnexpectedEof(usize, usize);

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            initial_total_bytes: bytes.len(),
            bytes,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn initial_total(&self) -> usize {
        self.initial_total_bytes
    }

    pub fn total_read(&self) -> usize {
        self.initial_total_bytes - self.bytes.len()
    }

    fn expected_more_bytes(&self, bytes: usize) -> UnexpectedEof {
        UnexpectedEof(self.total_read() + bytes, self.initial_total_bytes)
    }

    /// Consume a const number of bytes from the buffer, or return an error if the buffer doesn't have enough bytes.
    /// If the number of bytes to read is not a const, use [`split_off`] instead.
    pub fn read_exact<const N: usize>(&mut self) -> Result<[u8; N], UnexpectedEof> {
        // There isn't any chunks-type method that shrinks the input slice like `split_off`, so we must do this manually
        let bytes = self
            .bytes
            .split_off(..N)
            .ok_or_else(|| self.expected_more_bytes(N))?;
        let mut buf = [0; N];
        buf.copy_from_slice(bytes);
        Ok(buf)
    }

    /// Consume an exact amount of bytes from the buffer, or return an error if the buffer doesn't have enough bytes.
    /// If the number of bytes to read is a constant value, consider using [`read_exact`] to get an array back instead.
    pub fn split_off(&mut self, bytes: usize) -> Result<Self, UnexpectedEof> {
        self.bytes
            .split_off(..bytes)
            .ok_or_else(|| self.expected_more_bytes(bytes))
            .map(Self::new)
    }

    /// Consume the entire buffer.
    pub fn take_all(&mut self) -> &'a [u8] {
        std::mem::take(&mut self.bytes)
    }
}

impl<'a> From<&'a [u8]> for Reader<'a> {
    fn from(bytes: &'a [u8]) -> Self {
        Self::new(bytes)
    }
}

/// Does this type have a constant size (in bytes) when serialized, or is it variable?
/// If `SIZE_HINT` is `Some(_)`, types should always consume exactly that many bytes from the input buffer in their `FromBytes`
/// impl. If it is `None`, types should instead consume the entire buffer.
pub trait SizeHint {
    const SIZE_HINT: Option<usize>;
}

impl<T: SizeHint> SizeHint for &T {
    const SIZE_HINT: Option<usize> = T::SIZE_HINT;
}

impl<T: SizeHint> SizeHint for &mut T {
    const SIZE_HINT: Option<usize> = T::SIZE_HINT;
}

/// A bit like `AsRef<[u8]>`, but allows returning owned data too, which enables both directly returning some bytes without
/// copying, as well as serializing data into arbitrary bytes and returning that
pub trait AsBytes: SizeHint {
    type Bytes: HasBytes;
    fn as_bytes(&self) -> Bytes<'_, Self::Bytes>;
}

impl<T: AsBytes> AsBytes for &T {
    type Bytes = T::Bytes;

    fn as_bytes(&self) -> Bytes<'_, Self::Bytes> {
        T::as_bytes(self)
    }
}

impl<T: AsBytes> AsBytes for &mut T {
    type Bytes = T::Bytes;

    fn as_bytes(&self) -> Bytes<'_, Self::Bytes> {
        T::as_bytes(self)
    }
}

/// A bit like `TryFrom<&[u8]>`, but controlled by us to workaround the orphan rule (we need to impl deser logic for foreign types)
pub trait FromBytes: SizeHint + Sized {
    type Error;
    fn try_from(bytes: &mut Reader) -> Result<Self, Self::Error>;
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
        let mut bytes = self.as_ref().into();
        T::try_from(&mut bytes)
            .map_err(BytesIntoError::Deser)
            .and_then(|result| {
                bytes
                    .is_empty()
                    .then_some(result)
                    .ok_or_else(|| BytesIntoError::ExpectedEof(bytes.len(), bytes.initial_total()))
            })
    }
}

/// Helper trait for key prefix lookup
pub trait Prefixable<T: ?Sized> {
    type Prefix: HasBytes;
    fn prefix(prefix: &T) -> Self::Prefix;
}

impl<T, U: Prefixable<T>> Prefixable<T> for &U {
    type Prefix = U::Prefix;

    fn prefix(prefix: &T) -> Self::Prefix {
        U::prefix(prefix)
    }
}

impl<T, U: Prefixable<T>> Prefixable<T> for &mut U {
    type Prefix = U::Prefix;

    fn prefix(prefix: &T) -> Self::Prefix {
        U::prefix(prefix)
    }
}

/// Trait with bounds that enforce that the implementing type can be serialized and deserialized, which
/// makes it usable as a key or value in untyped KV databases, which only deal in `&[u8]`.
pub trait Serde: AsBytes + FromBytes {}

impl<T> Serde for T where T: AsBytes + FromBytes {}

/// Like a `Vec<String>` but where all the strings are stored inline in a single heap allocation.
#[derive(Default)]
pub struct InlineStrVec {
    // The format this uses is little-endian `u32` length prefixes for each string in the buffer
    buffer: Buffer,
}

impl InlineStrVec {
    pub fn empty() -> Self {
        Self {
            buffer: Buffer::empty(),
        }
    }

    pub fn iter(&self) -> InlineStrVecIter<'_> {
        InlineStrVecIter {
            reader: Reader::new(&self.buffer),
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
pub enum InlineStrVecError {
    #[error("unexpected EOF while reading length prefix")]
    LengthPrefixEof(#[source] UnexpectedEof),
    #[error("unexpected EOF while reading string")]
    StringEof(#[source] UnexpectedEof),
    #[error(transparent)]
    InvalidUTF8(#[from] Utf8Error),
}

impl SizeHint for InlineStrVec {
    const SIZE_HINT: Option<usize> = None;
}

impl FromBytes for InlineStrVec {
    type Error = InlineStrVecError;

    // It's important for this to check that the buffer is well-formed so the iterator impl can be infallible.
    fn try_from(bytes: &mut Reader) -> Result<Self, Self::Error> {
        // We get a hold of the entire buffer, then reconstruct the reader, because the reading we do is only for
        // checking correctness, and we really want to just copy the entire buffer all at once, afterwards.
        // Shared refs are `Copy`, so the new reader has its own copy of the buffer ref, separate from the one in `buffer`
        let buffer = bytes.take_all();
        let mut bytes = Reader::new(buffer);

        while !bytes.is_empty() {
            // The format is a little-endian u32, followed by a string of that many bytes, repeated to the end of the buffer
            let prefix = bytes
                .read_exact()
                .map_err(InlineStrVecError::LengthPrefixEof)?;
            let len = u32::from_le_bytes(prefix) as _;

            let string = bytes
                .split_off(len)
                .map_err(InlineStrVecError::StringEof)?
                .take_all();

            // We just need to check that the bytes in the buffer are valid UTF-8, doing anything with the str is unneeded
            let _ = str::from_utf8(string)?;
        }

        Ok(Self {
            buffer: buffer.into(),
        })
    }
}

impl AsBytes for InlineStrVec {
    type Bytes = Buffer;

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
    reader: Reader<'a>,
}

impl<'a> Iterator for InlineStrVecIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.reader.is_empty() {
            return None;
        }

        // We use unwraps here because `InlineStrVec` ensures it is well-formed on construction, so it should be fine
        let len = self.reader.read_exact().unwrap();
        let len = u32::from_le_bytes(len) as _;
        let string = self.reader.split_off(len).unwrap().take_all();
        Some(str::from_utf8(string).unwrap())
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
pub trait ConstantSize {
    /// hack
    const SIZE: usize;
}

/// hack
impl<T: SizeHint> ConstantSize for T {
    /// hack
    /// The unwrap being in a const expr makes it a compile error instead of a runtime panic
    const SIZE: usize = T::SIZE_HINT.unwrap();
}

/// Only supported for element types with constant serialized size
impl<T> AsBytes for SmallVec<T>
where
    T: AsBytes + ConstantSize,
{
    type Bytes = Buffer;

    fn as_bytes(&self) -> Bytes<'_, Self::Bytes> {
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
    T: FromBytes + ConstantSize,
{
    type Error = T::Error;

    fn try_from(bytes: &mut Reader) -> Result<Self, T::Error> {
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
    type Bytes = Buffer;

    fn as_bytes(&self) -> Bytes<'_, Self::Bytes> {
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

/// Naïvely using the first element directly for a prefix search would fail if it has a variable size, due to the length prefix
impl<T: AsBytes, U> Prefixable<T> for (T, U) {
    type Prefix = Buffer;

    fn prefix(t: &T) -> Self::Prefix {
        let t = t.as_bytes();
        if T::SIZE_HINT.is_none() {
            // If `T` has a variable size, it needs a length prefix
            let len = (t.len() as u32).to_le_bytes();
            ByteView::fused(len.as_ref(), t.as_ref()).into()
        } else {
            t.as_ref().into()
        }
    }
}

#[derive(Error)]
pub enum TupleError<T: FromBytes, U: FromBytes> {
    // None of these can be `#[from]` because ugh (see BytesIntoError *AND* DbError)
    #[error("unexpected EOF while reading length prefix")]
    LengthPrefixEof(#[source] UnexpectedEof),
    #[error("unexpected EOF while deserializing first tuple element")]
    FirstEof(#[source] UnexpectedEof),
    #[error("error deserializing first tuple element")]
    First(#[source] T::Error),
    #[error("error deserializing second tuple element")]
    Second(#[source] U::Error),
}

impl<T: FromBytes, U: FromBytes> FromBytes for (T, U) {
    type Error = TupleError<T, U>;

    fn try_from(bytes: &mut Reader) -> Result<Self, Self::Error> {
        if T::SIZE_HINT.is_none() {
            // If `T` has a variable size, we have a length prefix
            let prefix = bytes.read_exact().map_err(TupleError::LengthPrefixEof)?;
            let len = u32::from_le_bytes(prefix) as _;
            let mut t = bytes.split_off(len).map_err(TupleError::FirstEof)?;

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

    fn try_from(bytes: &mut Reader) -> Result<Self, Self::Error> {
        Ok(bytes.take_all().into())
    }
}
