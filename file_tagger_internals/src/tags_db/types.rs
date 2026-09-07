use std::{
    cmp::Ordering,
    convert::Infallible,
    fmt::{Debug, Display, Formatter},
    ops::{Bound, RangeBounds},
    str::{FromStr, Utf8Error},
};

use byteview::ByteView;
use estr::Estr;
use scru64::{Scru64Id, id::RangeError};
use thiserror::Error;

use crate::{
    Buffer,
    serde::{
        AsBytes, Bytes, DerefProxy, FromBytes, LENGTH_PREFIX_BYTES, Prefixable, Reader, SizeHint,
        SmallVec, SmallVecError, UnexpectedEof, Writer,
    },
    tags_db::types::TagDataError::Numerical,
};

/// Identifies a single entry in the database, which can have many associated tags.
/// Value is stored in big-endian form for correct lexicographic ordering.
/// Used for non-tag entries
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct Entry(pub(super) Scru64Id);

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

impl FromStr for Tag {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.into()))
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

/// Can exactly represent any of Rust's range types.
///
/// Compared to `(Bound<T>, Bound<T>)`, this cannot express the start of the range being excluded.
/// That is intentional, as there is no corresponding syntax nor dedicated range type(s) for that.
///
/// # Example
/// ```
/// fn any_range<T>(range: AnyRange<T>) {
///    match range {
///        // `start..end` or `Range`
///        AnyRange(Some(start), Bound::Excluded(end)) => {},
///        // `start..` or `RangeFrom`
///        AnyRange(Some(start), Bound::Unbounded) => {},
///        // `..` or `RangeFull`
///        AnyRange(None, Bound::Unbounded) => {},
///        // `start..=end` or `RangeInclusive`
///        AnyRange(Some(start), Bound::Included(end)) => {},
///        // `..end` or `RangeTo`
///        AnyRange(None, Bound::Excluded(end)) => {},
///        // `..=end` or `RangeToInclusive`
///        AnyRange(None, Bound::Included(end)) => {},
///    }
/// }
/// ```
pub struct AnyRange<T>(Option<T>, Bound<T>);

impl<T> RangeBounds<T> for AnyRange<T> {
    fn start_bound(&self) -> Bound<&T> {
        let Self(start, _) = self;
        match start {
            Some(start) => Bound::Included(start),
            None => Bound::Unbounded,
        }
    }

    fn end_bound(&self) -> Bound<&T> {
        let Self(_, end) = self;
        end.as_ref()
    }
}

impl<T> SizeHint for AnyRange<T> {
    /// In order for this to be `Some` we would have to include T::SIZE_HINT-length strings of 0s for `None` and `Unbounded`
    const SIZE_HINT: Option<usize> = None;
}

impl<T: AsBytes> AsBytes for AnyRange<T> {
    type Bytes = Buffer;

    fn as_bytes(&self) -> Bytes<'_, Self::Bytes> {
        let Self(start, end) = self;

        let (start_discriminant, start_bytes) = match start {
            None => (0, None),
            Some(start) => (1, Some(start.as_bytes())),
        };

        let (end_discriminant, end_bytes) = match end {
            Bound::Included(end) => (0, Some(end.as_bytes())),
            Bound::Excluded(end) => (1, Some(end.as_bytes())),
            Bound::Unbounded => (2, None),
        };

        // start discriminant and end discriminant are 1 byte each
        let mut total_bytes = 2;

        // If start value is not present it contributes 0 bytes
        if let Some(start_bytes) = &start_bytes {
            if let Some(size) = T::SIZE_HINT {
                // start value is present and fixed size (no length prefix)
                total_bytes += size
            } else {
                // start value is present and variable size (yes length prefix)
                total_bytes += LENGTH_PREFIX_BYTES;
                total_bytes += start_bytes.len();
            }
        }

        if let Some(end_bytes) = &end_bytes {
            if let Some(size) = T::SIZE_HINT {
                // end value is present and fixed size (fast path thanks to reading from the const)
                total_bytes += size;
            } else {
                // end value is present and variable size (no length prefix because it assumes the rest of the buffer)
                total_bytes += end_bytes.len();
            }
        }

        // start discriminant + length prefix (if necessary) + start value (if present) + end discriminant + end value (if present)
        let mut writer = Writer::new(total_bytes);
        writer.write_one(start_discriminant);
        if let Some(start_bytes) = start_bytes {
            if T::SIZE_HINT.is_some() {
                writer.write(start_bytes);
            } else {
                writer.write_with_length_prefix(start_bytes);
            }
        }
        writer.write_one(end_discriminant);
        if let Some(end_bytes) = end_bytes {
            writer.write(end_bytes);
        }
        Bytes::Owned(writer.finish().unwrap())
    }
}

#[derive(Error)]
pub enum AnyRangeError<T: FromBytes> {
    #[error("unexpected EOF while deserializing range")]
    Eof(#[from] UnexpectedEof),
    #[error("range start discriminant can only be 0 or 1, but got {0} instead")]
    InvalidStartDiscriminant(u8),
    #[error("range end discriminant can only be 0, 1, or 2, but got {0} instead")]
    InvalidEndDiscriminant(u8),
    #[error("error while deserializing range")]
    Deser(#[source] T::Error),
}

// No `T: Debug` bound please and thank you
impl<T: FromBytes> Debug for AnyRangeError<T>
where
    T::Error: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Eof(eof) => f.debug_tuple("Eof").field(eof).finish(),
            Self::InvalidStartDiscriminant(discriminant) => f
                .debug_tuple("InvalidStartDiscriminant")
                .field(discriminant)
                .finish(),
            Self::InvalidEndDiscriminant(arg0) => {
                f.debug_tuple("InvalidEndDiscriminant").field(arg0).finish()
            }
            Self::Deser(discriminant) => f.debug_tuple("Deser").field(discriminant).finish(),
        }
    }
}

impl<T: FromBytes> FromBytes for AnyRange<T> {
    type Error = AnyRangeError<T>;

    fn try_from(bytes: &mut Reader) -> Result<Self, Self::Error> {
        let mut bad_reader = None;

        let start_discriminant = bytes.read_one()?;
        let start = match start_discriminant {
            0 => None,
            1 => {
                if T::SIZE_HINT.is_some() {
                    Some(T::try_from(bytes).map_err(AnyRangeError::Deser)?)
                } else {
                    let mut t = bytes.read_length_prefix_and_split_off()?;
                    let start = T::try_from(&mut t).map_err(AnyRangeError::Deser)?;
                    if !t.is_empty() {
                        bad_reader = Some(t);
                    }
                    Some(start)
                }
            }
            e => return Err(AnyRangeError::InvalidStartDiscriminant(e)),
        };

        let end_discriminant = bytes.read_one()?;
        let end = match end_discriminant {
            0 => Bound::Included(T::try_from(bytes).map_err(AnyRangeError::Deser)?),
            1 => Bound::Excluded(T::try_from(bytes).map_err(AnyRangeError::Deser)?),
            2 => Bound::Unbounded,
            e => return Err(AnyRangeError::InvalidEndDiscriminant(e)),
        };

        if let Some(bad_reader) = bad_reader {
            *bytes = bad_reader;
        }

        Ok(Self(start, end))
    }
}

/// The kind of data that a tag has
#[repr(u8)]
pub enum TagData {
    /// The tag has no associated data
    None,
    /// A UTF-8 string
    String,
    /// An enum, a fixed set of allowed UTF-8 strings.
    Enum(SmallVec<Estr>),
    /// A true/false boolean
    Boolean,
    /// Unsigned integer within a given range
    Unsigned(AnyRange<u64>),
    /// Signed integer within a given range
    Signed(AnyRange<i64>),
    /// Floating point number within a given range
    Float(AnyRange<f64>),
    /// An opaque binary blob
    Buffer,
}

impl TagData {
    pub fn discriminant(&self) -> u8 {
        // SAFETY: This is safe as the type has `#[repr(u8)]`
        // See <https://doc.rust-lang.org/stable/reference/items/enumerations.html#pointer-casting>
        unsafe { *(self as *const Self as *const u8) }
    }
}

impl SizeHint for TagData {
    /// This is only `None` thanks to the `Enum` variant
    const SIZE_HINT: Option<usize> = None;
}

impl AsBytes for TagData {
    type Bytes = Buffer;

    fn as_bytes(&self) -> Bytes<'_, Self::Bytes> {
        // All of the variants with data are serialized exactly the same way
        // but can't be folded into one block as the types are heterogeneous...
        macro_rules! serialize {
            ($discriminant:ident, $data:ident) => {
                Bytes::Owned(ByteView::fused(&[$discriminant], $data.as_bytes().as_ref()).into())
            };
        }

        let discriminant = self.discriminant();
        match self {
            Self::Enum(variants) => serialize!(discriminant, variants),
            Self::Unsigned(range) => serialize!(discriminant, range),
            Self::Signed(range) => serialize!(discriminant, range),
            Self::Float(range) => serialize!(discriminant, range),
            // Variants without data are just the discriminant
            _ => Bytes::Owned(Buffer::new(&[discriminant])),
        }
    }
}

#[derive(Debug, Error)]
pub enum TagDataError {
    #[error("unexpected EOF while deserializing tag data type")]
    Eof(#[from] UnexpectedEof),
    #[error(
        "tag data type discriminant can only be 0, 1, 2, 3, 4, 5, 6, or 7, but got {0} instead"
    )]
    InvalidDiscriminant(u8),
    #[error("error while deserializing tag data enum type")]
    Enum(#[from] SmallVecError<Estr>),
    #[error("error while deserializing tag data numerical type")]
    Numerical(#[source] AnyRangeError<u8>),
}

// Cheeky thing because we know all numerical types have the same deser error type
impl<T> From<AnyRangeError<T>> for TagDataError
where
    T: FromBytes<Error = UnexpectedEof>,
{
    fn from(e: AnyRangeError<T>) -> Self {
        use AnyRangeError::*;
        // Deconstructing and reconstructing to just change the monomorphization as all the inner types are the same
        Self::Numerical(match e {
            Eof(e) => Eof(e),
            InvalidStartDiscriminant(e) => InvalidStartDiscriminant(e),
            InvalidEndDiscriminant(e) => InvalidEndDiscriminant(e),
            Deser(e) => Deser(e),
        })
    }
}

impl FromBytes for TagData {
    type Error = TagDataError;

    fn try_from(bytes: &mut Reader) -> Result<Self, Self::Error> {
        // All of the variants with data are deserialized exactly the same way
        // but can't be folded into one block as the types are heterogeneous...
        macro_rules! deserialize {
            ($bytes:ident, $variant:expr) => {
                FromBytes::try_from($bytes)
                    .map($variant)
                    .map_err(Into::into)
            };
        }

        let discriminant = bytes.read_one()?;
        match discriminant {
            0 => Ok(Self::None),
            1 => Ok(Self::String),
            2 => deserialize!(bytes, Self::Enum),
            3 => Ok(Self::Boolean),
            4 => deserialize!(bytes, Self::Unsigned),
            5 => deserialize!(bytes, Self::Signed),
            6 => deserialize!(bytes, Self::Float),
            7 => Ok(Self::Buffer),
            e => Err(TagDataError::InvalidDiscriminant(e)),
        }
    }
}
