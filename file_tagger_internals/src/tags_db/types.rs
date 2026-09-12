use std::{
    cmp::Ordering,
    convert::Infallible,
    fmt::{Debug, Display, Formatter},
    ops::{Bound, RangeBounds},
    str::{FromStr, Utf8Error},
};

use byteview::ByteView;
use estr::Estr;
use scru64::Scru64Id;
use thiserror::Error;

use crate::{
    Buffer,
    serde::{
        AsBytes, BoolError, Bytes, BytesInto, BytesIntoError, DerefProxy, FromBytes,
        LENGTH_PREFIX_BYTES, Prefixable, Reader, SizeHint, SmallVec, SmallVecError, UnexpectedEof,
        Writer,
    },
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
    Range(#[from] scru64::id::RangeError<u64>),
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
    type Error = scru64::id::RangeError<u64>;

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
#[derive(Debug, Clone, Copy)]
pub struct AnyRange<T>(Option<T>, Bound<T>);

#[derive(Debug, Error)]
#[error("{0} is outside the range {1}")]
pub struct RangeError<T>(T, AnyRange<T>);

impl<T: PartialOrd + Clone> AnyRange<T> {
    pub fn validate_range(&self, element: T) -> Result<T, RangeError<T>> {
        if self.contains(&element) {
            Ok(element)
        } else {
            Err(RangeError(element, self.clone()))
        }
    }
}

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

impl<T: Display> Display for AnyRange<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self(Some(start), Bound::Excluded(end)) => write!(f, "{start}..{end}"),
            Self(Some(start), Bound::Unbounded) => write!(f, "{start}.."),
            Self(None, Bound::Unbounded) => write!(f, ".."),
            Self(Some(start), Bound::Included(end)) => write!(f, "{start}..={end}"),
            Self(None, Bound::Excluded(end)) => write!(f, "..{end}"),
            Self(None, Bound::Included(end)) => write!(f, "..={end}"),
        }
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
///
/// The discriminants (including their order!) should be kept in sync with `TagData`
#[repr(u8)]
pub enum TagDataType {
    /// The tag has no associated data
    None,
    /// A UTF-8 string
    String,
    /// An enum, a fixed set of allowed UTF-8 strings.
    Enum(SmallVec<Estr>),
    /// A true/false boolean
    Bool,
    /// Unsigned integer within a given range
    Unsigned(AnyRange<u64>),
    /// Signed integer within a given range
    Signed(AnyRange<i64>),
    /// Floating point number within a given range
    Float(AnyRange<f64>),
    /// An opaque binary blob
    Buffer,
}

impl TagDataType {
    pub fn discriminant(&self) -> u8 {
        // SAFETY: This is safe as the type has `#[repr(u8)]`
        // See <https://doc.rust-lang.org/stable/reference/items/enumerations.html#pointer-casting>
        unsafe { *(self as *const Self as *const u8) }
    }

    pub fn type_matches(&self, data: &TagData) -> bool {
        // This relies on the order of the discriminants being the same
        self.discriminant() == data.discriminant()
    }

    /// Checks that the data is of the correct type, and that it is within the allowed set of values (for `Enum` and numerical)
    pub fn verify_data(&self, data: &TagData) -> Result<(), ()> {
        if !self.type_matches(data) {
            return Err(());
        }

        let valid = match (self, data) {
            (TagDataType::Enum(variants), TagData::Enum(value)) => variants.contains(value),
            (TagDataType::Unsigned(range), TagData::Unsigned(u)) => range.contains(u),
            (TagDataType::Signed(range), TagData::Signed(i)) => range.contains(i),
            (TagDataType::Float(range), TagData::Float(f)) => range.contains(f),
            // `type_matches` verifies every other variant
            _ => true,
        };

        valid.ok_or(())
    }
}

impl SizeHint for TagDataType {
    /// This is only `None` thanks to the `Enum` variant
    const SIZE_HINT: Option<usize> = None;
}

impl AsBytes for TagDataType {
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
pub enum TagDataTypeError {
    #[error("unexpected EOF while deserializing tag data type")]
    Eof(#[from] UnexpectedEof),
    #[error(
        "tag data type discriminant can only be 0, 1, 2, 3, 4, 5, 6, or 7, but got {0} instead"
    )]
    InvalidDiscriminant(u8),
    #[error("error while deserializing tag data enum type variants")]
    Enum(#[from] SmallVecError<Estr>),
    #[error("error while deserializing tag data numerical type range")]
    Numerical(#[source] AnyRangeError<u8>),
}

// Cheeky thing because we know all numerical types have the same deser error type
impl<T> From<AnyRangeError<T>> for TagDataTypeError
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

impl FromBytes for TagDataType {
    type Error = TagDataTypeError;

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
            3 => Ok(Self::Bool),
            4 => deserialize!(bytes, Self::Unsigned),
            5 => deserialize!(bytes, Self::Signed),
            6 => deserialize!(bytes, Self::Float),
            7 => Ok(Self::Buffer),
            e => Err(TagDataTypeError::InvalidDiscriminant(e)),
        }
    }
}

/// A strongly-typed instance of tag data containing the deserialized value
///
/// The discriminants (including their order!) should be kept in sync with `TagDataType`
#[repr(u8)]
pub enum TagData {
    /// The tag has no associated data.
    ///
    /// This variant cannot ever be constructed, and exists only to ensure the discriminant values match `TagDataType`
    None(Infallible),
    /// A UTF-8 string
    String(String),
    /// An enum, a fixed set of allowed UTF-8 strings.
    Enum(Estr),
    /// A true/false boolean
    Bool(bool),
    /// Unsigned integer within a given range
    Unsigned(u64),
    /// Signed integer within a given range
    Signed(i64),
    /// Floating point number within a given range
    Float(f64),
    /// An opaque binary blob
    Buffer(Buffer),
}

impl TagData {
    pub fn discriminant(&self) -> u8 {
        // SAFETY: This is safe as the type has `#[repr(u8)]`
        // See <https://doc.rust-lang.org/stable/reference/items/enumerations.html#pointer-casting>
        unsafe { *(self as *const Self as *const u8) }
    }
}

/// hack
impl SizeHint for TagData {
    const SIZE_HINT: Option<usize> = unreachable!();
}

/// hack
impl FromBytes for TagData {
    type Error = TagDataError;

    fn try_from(_: &mut Reader) -> Result<Self, Self::Error> {
        unreachable!()
    }
}

impl AsBytes for TagData {
    type Bytes = Buffer;

    fn as_bytes(&self) -> Bytes<'_, Self::Bytes> {
        // We don't need to encode our discriminant as that is stored in the corresponding `TagDataType` instance for this tag
        match self {
            TagData::None(nuh_uh) => match *nuh_uh {},
            TagData::String(s) => Bytes::Borrowed(s.as_bytes()),
            TagData::Enum(s) => Bytes::Borrowed(s.as_str().as_bytes()),
            TagData::Bool(b) => b.as_bytes(),
            TagData::Unsigned(u) => u.as_bytes(),
            TagData::Signed(i) => i.as_bytes(),
            TagData::Float(f) => f.as_bytes(),
            TagData::Buffer(buf) => buf.as_bytes(),
        }
    }
}

/// Because `SmallVec` (and `Vec`) don't impl `Display`...
///
/// Note: This wraps the displayed elements in '' so it's only appropriate for stringy types
macro_rules! fmt_vec {
    ($vec:expr) => {
        std::fmt::from_fn(move |f| {
            use std::fmt::Write;
            f.write_char('[')?;
            f.write_char(' ')?;
            let mut iter = $vec.iter();
            let mut next = iter.next();
            // `Iterator::intersperse` is nightly so we do it manually
            while let Some(s) = next {
                f.write_char('\'')?;
                std::fmt::Display::fmt(s, f)?;
                f.write_char('\'')?;
                next = iter.next();
                if next.is_some() {
                    f.write_char(',')?;
                    f.write_char(' ')?;
                }
            }
            f.write_char(' ')?;
            f.write_char(']')
        })
    };
}

#[derive(Debug, Error)]
pub enum TagDataError {
    #[error("tag must have data to deserialize")]
    None,
    #[error("error while deserializing string data")]
    String(#[source] Utf8Error),
    #[error("error while deserializing enum data")]
    EnumDeser(#[source] Utf8Error),
    #[error("enum data '{0}' is outside the allowed set of values: {v}", v = fmt_vec!(.1))]
    EnumVariant(Estr, Vec<Estr>),
    #[error("error while deserializing bool data")]
    Bool(#[from] BoolError),
    #[error("error while deserializing numerical data")]
    NumericalDeser(#[from] UnexpectedEof),
    // These can't be folded into a single generic "numerical range" variant
    // because the different number types need to be formatted differently
    #[error("unsigned integer data is outside the permitted range")]
    UnsignedRange(#[from] RangeError<u64>),
    #[error("signed integer data is outside the permitted range")]
    SignedRange(#[from] RangeError<i64>),
    #[error("floating point data is outside the permitted range")]
    FloatRange(#[from] RangeError<f64>),
}

impl TagData {
    /// Deserialize the raw tag data to a strongly-typed value, according to the provided type metadata
    pub fn deserialize(buf: Buffer, metadata: &TagDataType) -> Result<Self, BytesIntoError<Self>> {
        macro_rules! deserialize {
            ($buf:ident, $variant:ident, $deser_err:ident $(, $validate:expr)?) => {
                $buf.bytes_into()
                    .map_err(|e| match e {
                        BytesIntoError::ExpectedEof(rem, total) => BytesIntoError::ExpectedEof(rem, total),
                        BytesIntoError::Deser(e) => BytesIntoError::Deser(TagDataError::$deser_err(e)),
                    })
                    $( .and_then(|data| $validate(data).map_err(|e| BytesIntoError::Deser(e))) )?
                    .map(Self::$variant)
            };
        }

        match metadata {
            TagDataType::None => Err(BytesIntoError::Deser(TagDataError::None)),
            TagDataType::String => deserialize!(buf, String, String),
            TagDataType::Enum(variants) => {
                deserialize!(buf, Enum, EnumDeser, |s| variants
                    .contains(&s)
                    .then_some(s)
                    .ok_or_else(|| TagDataError::EnumVariant(s, variants.to_vec())))
            }
            TagDataType::Bool => deserialize!(buf, Bool, Bool),
            TagDataType::Unsigned(range) => {
                deserialize!(buf, Unsigned, NumericalDeser, |i| range
                    .validate_range(i)
                    .map_err(TagDataError::UnsignedRange))
            }
            TagDataType::Signed(range) => {
                deserialize!(buf, Signed, NumericalDeser, |i| range
                    .validate_range(i)
                    .map_err(TagDataError::SignedRange))
            }
            TagDataType::Float(range) => {
                deserialize!(buf, Float, NumericalDeser, |i| range
                    .validate_range(i)
                    .map_err(TagDataError::FloatRange))
            }
            TagDataType::Buffer => Ok(Self::Buffer(buf)),
        }
    }
}
