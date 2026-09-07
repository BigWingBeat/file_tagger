use std::ops::Deref;

use byteview::{Builder, ByteView};
use thiserror::Error;

mod impls;

pub use impls::{SmallVecError, TupleError};

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

impl<B: HasBytes + Default> Default for Bytes<'_, B> {
    fn default() -> Self {
        Self::Owned(B::default())
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

    /// Consume one (1) byte from the buffer, or return an error if the buffer doesn't have enough bytes.
    pub fn read_one(&mut self) -> Result<u8, UnexpectedEof> {
        self.bytes
            .split_off_first()
            .copied()
            .ok_or_else(|| self.expected_more_bytes(1))
    }

    /// Consume a const number of bytes from the buffer, or return an error if the buffer doesn't have enough bytes.
    /// If the number of bytes to read is not a const, use [`split_off`] instead.
    pub fn read_exact<const N: usize>(&mut self) -> Result<[u8; N], UnexpectedEof> {
        // There isn't any chunks-type method that shrinks the input slice like `split_off`, so we must do this manually
        if let Some((&chunk, remainder)) = self.bytes.split_first_chunk() {
            self.bytes = remainder;
            Ok(chunk)
        } else {
            Err(self.expected_more_bytes(N))
        }
    }

    /// Consume a little-endian u32 length prefix from the buffer, or return an error if the buffer doesn't have enough bytes.
    pub fn read_length_prefix(&mut self) -> Result<usize, UnexpectedEof> {
        self.read_exact().map(|b| u32::from_le_bytes(b) as _)
    }

    /// Consume an exact amount of bytes from the buffer, or return an error if the buffer doesn't have enough bytes.
    /// If the number of bytes to read is a constant value, consider using [`read_exact`] to get an array back instead.
    pub fn split_off(&mut self, bytes: usize) -> Result<Self, UnexpectedEof> {
        self.bytes
            .split_off(..bytes)
            .ok_or_else(|| self.expected_more_bytes(bytes))
            .map(Self::new)
    }

    /// Consume a little-endian u32 length prefix from the buffer, then consume and return that many bytes from the buffer,
    /// or return an error if the buffer doesn't have enough bytes.
    pub fn read_length_prefix_and_split_off(&mut self) -> Result<Self, UnexpectedEof> {
        self.read_length_prefix().and_then(|i| self.split_off(i))
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

/// Serialization helper for `AsBytes` implementors.
///
/// This type keeps track of the number of written bytes, and upon [`finish`] being called, will return `Err`
/// if too few or too many bytes were written, compared to the `size` that was specified on construction.
///
/// We do it this way, instead of having the `write` methods fail eagerly, to reduce the amount
/// of (possibly panicking) branches that are generated in the middle of serialization code.
pub struct Writer {
    buffer: Builder,
    written: usize,
}

#[derive(Error, Debug)]
#[error(
    "buffer was allocated with {} bytes but {}",
    self.0,
    if self.1 == usize::MAX { "more bytes than that were written" } else { "only {self.1} bytes were written" }
)]
pub struct IncorrectBufferSize(usize, usize);

impl Writer {
    /// Allocate exactly `size` bytes to be written
    #[inline]
    pub fn new(size: usize) -> Self {
        Self {
            buffer: ByteView::builder(size),
            written: 0,
        }
    }

    #[inline]
    fn try_write(&mut self, bytes: &[u8]) {
        let range_end = self.written.saturating_add(bytes.len());
        if range_end > self.buffer.len() {
            // Sentinel value to indicate that writes have exceeded our total len.
            // Once this happens once, we will always enter this branch every time
            self.written = usize::MAX;
        } else {
            // SAFETY: We check that `a.saturating_add(b)` is in-bounds, which implies that `a` and `b` are both also in-bounds
            unsafe {
                let dst = self.buffer.as_mut_ptr().offset(self.written as _);
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, bytes.len());
            }
            self.written = range_end;
        }
    }

    // Duplicate of `try_write` with const size for arrays, for better codegen
    // See: <https://github.com/rust-lang/rust/blob/752b9bf8798c2ffc1d3fe2b804c04454366fc6d6/library/proc_macro/src/bridge/buffer.rs#L50-L53>
    #[inline]
    fn try_write_array<const N: usize>(&mut self, bytes: &[u8; N]) {
        let range_end = self.written.saturating_add(bytes.len());
        if range_end > self.buffer.len() {
            // Sentinel value to indicate that writes have exceeded our total len.
            // Once this happens once, we will always enter this branch every time
            self.written = usize::MAX;
        } else {
            // SAFETY: We check that `a.saturating_add(b)` is in-bounds, which implies that `a` and `b` are both also in-bounds
            unsafe {
                let dst = self.buffer.as_mut_ptr().offset(self.written as _);
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, bytes.len());
            }
            self.written = range_end;
        }
    }

    /// Write 1 byte
    #[inline]
    pub fn write_one(&mut self, byte: u8) {
        self.try_write_array(&[byte]);
    }

    /// Write some bytes
    #[inline]
    pub fn write(&mut self, bytes: impl AsRef<[u8]>) {
        let bytes = bytes.as_ref();
        self.try_write(bytes);
    }

    /// Write a constant number of bytes
    #[inline]
    pub fn write_fixed<const N: usize>(&mut self, bytes: &[u8; N]) {
        self.try_write_array(bytes);
    }

    /// Write a little-endian u32 length prefix, followed by that many bytes
    #[inline]
    pub fn write_with_length_prefix(&mut self, bytes: impl AsRef<[u8]>) {
        let bytes = bytes.as_ref();
        let length_prefix = (bytes.len() as u32).to_le_bytes();
        self.try_write_array(&length_prefix);
        self.try_write(bytes);
    }

    /// Return the written bytes, or an error if the number of written bytes did not match the number of initially allocated bytes
    #[inline]
    pub fn finish(self) -> Result<Buffer, IncorrectBufferSize> {
        if self.written != self.buffer.len() {
            Err(IncorrectBufferSize(self.written, self.buffer.len()))
        } else {
            Ok(self.buffer.freeze().into())
        }
    }
}

pub const LENGTH_PREFIX_BYTES: usize = size_of::<u32>();

/// Does this type have a constant size (in bytes) when serialized, or is it variable?
/// If `SIZE_HINT` is `Some(_)`, types should always consume exactly that many bytes from the input buffer in their `FromBytes`
/// impl. If it is `None`, types should instead consume the entire buffer.
///
/// This is so that we can elide redundant length prefixes in cases where it's just the Entire Buffer.
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
