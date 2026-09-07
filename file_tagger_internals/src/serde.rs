use std::ops::Deref;

use byteview::{Builder, ByteView};
use thiserror::Error;

mod impls;

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
