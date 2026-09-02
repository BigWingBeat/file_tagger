use std::{
    convert::Infallible,
    io::Write,
    ops::{Deref, DerefMut},
    str::Utf8Error,
};

use byteview::ByteView;
use estr::Estr;
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

// Serde impls for commonly used types

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

impl SizeHint for Estr {
    const SIZE_HINT: Option<usize> = None;
}

impl FromBytes for Estr {
    type Error = Utf8Error;

    fn try_from(bytes: &mut Reader) -> Result<Self, Self::Error> {
        str::from_utf8(bytes.take_all()).map(Estr::from)
    }
}

impl AsBytes for Estr {
    type Bytes = String;

    fn as_bytes(&self) -> Bytes<'_, Self::Bytes> {
        Bytes::Borrowed(self.as_str())
    }
}

impl<T: AsRef<str> + ?Sized> Prefixable<T> for Estr {
    type Prefix = String;

    fn prefix(prefix: &T) -> Self::Prefix {
        prefix.as_ref().to_owned()
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

pub type SmallSortedSet<T> = small_sorted_set::SmallSortedSet<T, INLINE_SIZE>;

impl<T> SizeHint for SmallVec<T> {
    const SIZE_HINT: Option<usize> = None;
}

impl<T> AsBytes for SmallVec<T>
where
    T: AsBytes,
{
    type Bytes = Buffer;

    fn as_bytes(&self) -> Bytes<'_, Self::Bytes> {
        if let Some(size) = T::SIZE_HINT {
            let total_bytes = size * self.len();
            let mut buffer = ByteView::builder(total_bytes);
            // We would use `as_chunks_mut` here but we can't because of bullshit
            for (element, chunk) in self.iter().zip(buffer.chunks_exact_mut(size)) {
                let bytes = element.as_bytes();
                chunk.copy_from_slice(bytes.as_ref());
            }
            Bytes::Owned(buffer.freeze().into())
        } else {
            // This `else` block means our element type has a variable serialized size, so each element needs a length prefix
            let total_length_prefix_bytes = self.len() * std::mem::size_of::<u32>();

            // `as_bytes` is only expensive if it does a heap allocation, which is rare (only this and tuples do that).
            // Most impls are cheap reference-to-reference conversions, or just write out fixed-size arrays on the stack.
            // So, serializing all of our elements twice is most likely not that costly.
            let total_bytes: usize = self.iter().map(|element| element.as_bytes().len()).sum();
            let total_bytes = total_bytes + total_length_prefix_bytes;
            let mut bytes = ByteView::builder(total_bytes);

            let mut buffer = bytes.deref_mut();
            for element in self.iter() {
                let bytes = element.as_bytes();
                let length_prefix = (bytes.len() as u32).to_le_bytes();
                // The `std::io::write` impl for byte slices is not a black box, unlike with
                // most real IO, so we know that it should never return any errors in this case.
                buffer.write_all(length_prefix.as_slice()).unwrap();
                buffer.write_all(bytes.as_ref()).unwrap();
            }

            Bytes::Owned(bytes.freeze().into())
        }
    }
}

#[derive(Error, Debug)]
pub enum SmallVecError<T: FromBytes> {
    #[error("unexpected EOF while reading length prefix")]
    LengthPrefixEof(#[source] UnexpectedEof),
    #[error("unexpected EOF while deserializing list element")]
    ElementEof(#[source] UnexpectedEof),
    #[error("error deserializing list element")]
    Deser(#[source] T::Error),
}

impl<T> FromBytes for SmallVec<T>
where
    T: FromBytes,
{
    type Error = SmallVecError<T>;

    fn try_from(bytes: &mut Reader) -> Result<Self, Self::Error> {
        if let Some(size) = T::SIZE_HINT {
            let elements = bytes.len() / size;
            let mut vec = Self::with_capacity(elements);
            while !bytes.is_empty() {
                let element = T::try_from(bytes).map_err(SmallVecError::Deser)?;
                vec.push(element);
            }
            // No `shrink_to_fit` in this case because here we're able to allocate the exact number of elements up-front,
            // so we won't be invoking the speculative overcompensating reallocation behaviour.
            Ok(vec)
        } else {
            let mut vec = Self::new();
            let mut bad_reader = None;
            while !bytes.is_empty() {
                let prefix = bytes.read_exact().map_err(SmallVecError::LengthPrefixEof)?;
                let len = u32::from_le_bytes(prefix) as _;
                let mut t = bytes.split_off(len).map_err(SmallVecError::ElementEof)?;
                let element = T::try_from(&mut t).map_err(SmallVecError::Deser)?;
                vec.push(element);

                if !t.is_empty() {
                    // If `T` doesn't actually consume all of its bytes, we make that visible here
                    bad_reader = Some(t);
                }
            }

            if let Some(bad_reader) = bad_reader {
                *bytes = bad_reader;
            }

            vec.shrink_to_fit();
            Ok(vec)
        }
    }
}

impl<T> SizeHint for SmallSortedSet<T> {
    const SIZE_HINT: Option<usize> = None;
}

impl<T: AsBytes> AsBytes for SmallSortedSet<T> {
    type Bytes = Buffer;

    fn as_bytes(&self) -> Bytes<'_, Self::Bytes> {
        self.as_vec().as_bytes()
    }
}

impl<T: FromBytes + Ord> FromBytes for SmallSortedSet<T> {
    type Error = SmallVecError<T>;

    fn try_from(bytes: &mut Reader) -> Result<Self, Self::Error> {
        let vec = FromBytes::try_from(bytes)?;
        Ok(Self::from_unsorted(vec))
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
