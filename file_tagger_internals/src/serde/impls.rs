use std::{convert::Infallible, str::Utf8Error};

use byteview::ByteView;
use estr::Estr;
use small_sorted_set::SmallSortedSet;
use smallvec::SmallVec;
use thiserror::Error;

use super::{
    AsBytes, Buffer, Bytes, DerefProxy, FromBytes, LENGTH_PREFIX_BYTES, Prefixable, Reader,
    SizeHint, UnexpectedEof, Writer,
};

/* Primitive numerical types */

macro_rules! impl_serde_numerical {
    ($($ty:ty),* $(,)*) => {
        $(
            impl SizeHint for $ty {
                const SIZE_HINT: Option<usize> = Some(size_of::<Self>());
            }

            impl AsBytes for $ty {
                type Bytes = Buffer;

                fn as_bytes(&self) -> Bytes<'_, Self::Bytes> {
                    // SAFETY: primitive numerical types are POD and as such are always safe to cast to raw bytes
                    // See also: <https://doc.rust-lang.org/stable/src/core/num/uint_macros.rs.html#4016-4017>
                    let bytes = unsafe { std::slice::from_raw_parts(std::ptr::from_ref(self) as *const u8, size_of::<Self>()) };
                    // The safe alternative would be using `to_ne_bytes()` and returning the array as `Bytes::Owned`,
                    // but that makes these impls more annoying to use for Reasons
                    Bytes::Borrowed(bytes)
                }
            }

            impl FromBytes for $ty {
                type Error = UnexpectedEof;

                fn try_from(bytes: &mut Reader) -> Result<Self, Self::Error> {
                    bytes.read_exact().map(Self::from_ne_bytes)
                }
            }
        )*
    };
}

impl_serde_numerical!(u8, u16, u32, u64, u128, i8, i16, i32, i64, i128, f32, f64);

#[cfg(target_endian = "big")]
compile_error!(
    "The serde impl for numerical types uses native-endian encoding, meaning databases are not portable between platforms with different endianness."
);

/* bool */

impl SizeHint for bool {
    const SIZE_HINT: Option<usize> = Some(size_of::<Self>());
}

impl AsBytes for bool {
    type Bytes = Buffer;

    fn as_bytes(&self) -> Bytes<'_, Self::Bytes> {
        // SAFETY: `bool` is really just a `u8` that is always either `0` or `1`,
        // so this is always safe for the same reason that this is safe for the integer types
        let bytes = unsafe {
            std::slice::from_raw_parts(std::ptr::from_ref(self) as *const u8, size_of::<Self>())
        };
        Bytes::Borrowed(bytes)
    }
}

#[derive(Debug, Error)]
pub enum BoolError {
    #[error("unexpected EOF while deserializing bool")]
    Eof(#[from] UnexpectedEof),
    #[error("bool value must be 0 or 1 but was {0} instead")]
    Deser(u8),
}

impl FromBytes for bool {
    type Error = BoolError;

    fn try_from(bytes: &mut Reader) -> Result<Self, Self::Error> {
        bytes.read_one().map_err(Into::into).and_then(|b| match b {
            2.. => Err(BoolError::Deser(b)),
            _ => Ok(b != 0),
        })
    }
}

/* Arrays of bytes */

impl<const N: usize> SizeHint for [u8; N] {
    const SIZE_HINT: Option<usize> = Some(N);
}

impl<const N: usize> FromBytes for [u8; N] {
    type Error = UnexpectedEof;

    fn try_from(bytes: &mut Reader) -> Result<Self, Self::Error> {
        bytes.read_exact()
    }
}

impl<const N: usize> AsBytes for [u8; N] {
    type Bytes = DerefProxy<Self>;

    fn as_bytes(&self) -> Bytes<'_, Self::Bytes> {
        Bytes::Borrowed(self.as_slice())
    }
}

/* Buffer */

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

/* String */

impl SizeHint for String {
    const SIZE_HINT: Option<usize> = None;
}

impl AsBytes for String {
    type Bytes = Self;

    fn as_bytes(&self) -> Bytes<'_, Self::Bytes> {
        Bytes::Borrowed(self.as_str())
    }
}

impl FromBytes for String {
    type Error = Utf8Error;

    fn try_from(bytes: &mut Reader) -> Result<Self, Self::Error> {
        str::from_utf8(bytes.take_all()).map(Into::into)
    }
}

impl<T: AsRef<str> + ?Sized> Prefixable<T> for String {
    type Prefix = Self;

    fn prefix(prefix: &T) -> Self::Prefix {
        prefix.as_ref().to_owned()
    }
}

/* Estr */

impl SizeHint for Estr {
    const SIZE_HINT: Option<usize> = None;
}

impl AsBytes for Estr {
    type Bytes = String;

    fn as_bytes(&self) -> Bytes<'_, Self::Bytes> {
        Bytes::Borrowed(self.as_str())
    }
}

impl FromBytes for Estr {
    type Error = Utf8Error;

    fn try_from(bytes: &mut Reader) -> Result<Self, Self::Error> {
        str::from_utf8(bytes.take_all()).map(Estr::from)
    }
}

impl<T: AsRef<str> + ?Sized> Prefixable<T> for Estr {
    type Prefix = String;

    fn prefix(prefix: &T) -> Self::Prefix {
        prefix.as_ref().to_owned()
    }
}

/* SmallVec */

impl<T, const N: usize> SizeHint for SmallVec<[T; N]> {
    const SIZE_HINT: Option<usize> = None;
}

impl<T, const N: usize> AsBytes for SmallVec<[T; N]>
where
    T: AsBytes,
{
    type Bytes = Buffer;

    fn as_bytes(&self) -> Bytes<'_, Self::Bytes> {
        if let Some(size) = T::SIZE_HINT {
            let total_bytes = size * self.len();
            let mut writer = Writer::new(total_bytes);
            // We would use `as_chunks_mut` here but we can't because of bullshit
            for element in self {
                let bytes = element.as_bytes();
                writer.write(bytes);
            }
            Bytes::Owned(writer.finish().unwrap())
        } else {
            // This `else` block means our element type has a variable serialized size, so each element needs a length prefix
            let total_length_prefix_bytes = self.len() * LENGTH_PREFIX_BYTES;

            // `as_bytes` is only expensive if it does a heap allocation, which is rare (mostly just collection types do).
            // Most impls are cheap reference-to-reference conversions, or just write out fixed-size arrays on the stack.
            // So, serializing all of our elements twice is most likely not that costly.
            let total_bytes: usize = self.iter().map(|element| element.as_bytes().len()).sum();
            let total_bytes = total_bytes + total_length_prefix_bytes;

            let mut writer = Writer::new(total_bytes);
            for element in self {
                let bytes = element.as_bytes();
                writer.write_with_length_prefix(bytes);
            }

            Bytes::Owned(writer.finish().unwrap())
        }
    }
}

#[derive(Error)]
pub enum SmallVecError<T: FromBytes> {
    #[error("unexpected EOF while deserializing list element")]
    Eof(#[from] UnexpectedEof),
    #[error("error deserializing list element")]
    Deser(#[source] T::Error),
}

/// Trait booouuuuuunnnds...!
impl<T: FromBytes> std::fmt::Debug for SmallVecError<T>
where
    T::Error: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Eof(eof) => f.debug_tuple("Eof").field(eof).finish(),
            Self::Deser(e) => f.debug_tuple("Deser").field(e).finish(),
        }
    }
}

impl<T, const N: usize> FromBytes for SmallVec<[T; N]>
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
                let mut t = bytes.read_length_prefix_and_split_off()?;
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

/* SmallSortedSet */

impl<T, const N: usize> SizeHint for SmallSortedSet<T, N> {
    const SIZE_HINT: Option<usize> = None;
}

impl<T: AsBytes, const N: usize> AsBytes for SmallSortedSet<T, N> {
    type Bytes = Buffer;

    fn as_bytes(&self) -> Bytes<'_, Self::Bytes> {
        self.as_vec().as_bytes()
    }
}

impl<T: FromBytes + Ord, const N: usize> FromBytes for SmallSortedSet<T, N> {
    type Error = SmallVecError<T>;

    fn try_from(bytes: &mut Reader) -> Result<Self, Self::Error> {
        let vec = FromBytes::try_from(bytes)?;
        Ok(Self::from_unsorted(vec))
    }
}

/* 2-tuples */

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
            let total_bytes = LENGTH_PREFIX_BYTES + t.len() + u.len();
            let mut writer = Writer::new(total_bytes);
            writer.write_with_length_prefix(t);
            writer.write(u);
            Bytes::Owned(writer.finish().unwrap())
        } else {
            // A built-in method that does exactly what we're manually doing above, but only for 2 input slices
            Bytes::Owned(ByteView::fused(t.as_ref(), u.as_ref()).into())
        }
    }
}

#[derive(Error)]
pub enum TupleError<T: FromBytes, U: FromBytes> {
    #[error("unexpected EOF while deserializing first tuple element")]
    Eof(#[from] UnexpectedEof),
    #[error("error deserializing first tuple element")]
    First(#[source] T::Error),
    #[error("error deserializing second tuple element")]
    Second(#[source] U::Error),
}

/// You know...
impl<T: FromBytes, U: FromBytes> std::fmt::Debug for TupleError<T, U>
where
    T::Error: std::fmt::Debug,
    U::Error: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Eof(eof) => f.debug_tuple("Eof").field(eof).finish(),
            Self::First(e) => f.debug_tuple("First").field(e).finish(),
            Self::Second(e) => f.debug_tuple("Second").field(e).finish(),
        }
    }
}

impl<T: FromBytes, U: FromBytes> FromBytes for (T, U) {
    type Error = TupleError<T, U>;

    fn try_from(bytes: &mut Reader) -> Result<Self, Self::Error> {
        if T::SIZE_HINT.is_none() {
            // If `T` has a variable size, we have a length prefix
            let mut t = bytes.read_length_prefix_and_split_off()?;

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

#[cfg(test)]
mod test {
    use estr::Estr;
    use small_sorted_set::SmallSortedSet;
    use smallvec::{SmallVec, smallvec};

    use super::super::{AsBytes, Buffer, BytesInto};

    macro_rules! serde_roundtrip {
        ($ty:ty, $value:expr, $bytes:expr) => {{
            #[allow(clippy::type_complexity, reason = "shut up")]
            let value: $ty = $value;
            let bytes = value.as_bytes();
            assert_eq!(bytes.as_ref(), $bytes);
            #[allow(clippy::type_complexity, reason = "shut up")]
            let roundtrip: $ty = bytes.bytes_into().unwrap();
            assert_eq!(roundtrip, value);
        }};
    }

    #[test]
    fn primitives() {
        serde_roundtrip!(u8, 0, [0]);
        serde_roundtrip!(u8, 1, [1]);
        serde_roundtrip!(u8, 255, [255]);

        serde_roundtrip!(u16, 0, [0, 0]);
        serde_roundtrip!(u16, 1, [1, 0]);
        serde_roundtrip!(u16, 255, [255, 0]);
        serde_roundtrip!(u16, 256, [0, 1]);
        serde_roundtrip!(u16, 65280, [0, 255]);
        serde_roundtrip!(u16, 65535, [255, 255]);

        serde_roundtrip!(u32, 0, [0, 0, 0, 0]);
        serde_roundtrip!(u32, 1, [1, 0, 0, 0]);
        serde_roundtrip!(u32, 255, [255, 0, 0, 0]);
        serde_roundtrip!(u32, 256, [0, 1, 0, 0]);
        serde_roundtrip!(u32, 65280, [0, 255, 0, 0]);
        serde_roundtrip!(u32, 65536, [0, 0, 1, 0]);
        serde_roundtrip!(u32, 4294967295, [255, 255, 255, 255]);

        serde_roundtrip!(u64, 0, [0, 0, 0, 0, 0, 0, 0, 0]);
        serde_roundtrip!(u64, 1, [1, 0, 0, 0, 0, 0, 0, 0]);
        serde_roundtrip!(u64, 4294967296, [0, 0, 0, 0, 1, 0, 0, 0]);
        serde_roundtrip!(
            u64,
            18446744073709551615,
            [255, 255, 255, 255, 255, 255, 255, 255]
        );

        serde_roundtrip!(u128, 0, [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        serde_roundtrip!(u128, 1, [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        serde_roundtrip!(
            u128,
            18446744073709551616,
            [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]
        );
        serde_roundtrip!(
            u128,
            340282366920938463463374607431768211455,
            [
                255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            ]
        );

        serde_roundtrip!(i8, -128, [128]);
        serde_roundtrip!(i8, -127, [129]);
        serde_roundtrip!(i8, -1, [255]);
        serde_roundtrip!(i8, 0, [0]);
        serde_roundtrip!(i8, 1, [1]);
        serde_roundtrip!(i8, 127, [127]);

        serde_roundtrip!(i16, -32768, [0, 128]);
        serde_roundtrip!(i16, -32767, [1, 128]);
        serde_roundtrip!(i16, -1, [255, 255]);
        serde_roundtrip!(i16, 0, [0, 0]);
        serde_roundtrip!(i16, 1, [1, 0]);
        serde_roundtrip!(i16, 256, [0, 1]);
        serde_roundtrip!(i16, 32767, [255, 127]);

        serde_roundtrip!(i32, -2147483648, [0, 0, 0, 128]);
        serde_roundtrip!(i32, -2147483647, [1, 0, 0, 128]);
        serde_roundtrip!(i32, -1, [255, 255, 255, 255]);
        serde_roundtrip!(i32, 0, [0, 0, 0, 0]);
        serde_roundtrip!(i32, 1, [1, 0, 0, 0]);
        serde_roundtrip!(i32, 32768, [0, 128, 0, 0]);
        serde_roundtrip!(i32, 2147483647, [255, 255, 255, 127]);

        serde_roundtrip!(i64, -9223372036854775808, [0, 0, 0, 0, 0, 0, 0, 128]);
        serde_roundtrip!(i64, -9223372036854775807, [1, 0, 0, 0, 0, 0, 0, 128]);
        serde_roundtrip!(i64, -1, [255, 255, 255, 255, 255, 255, 255, 255]);
        serde_roundtrip!(i64, 0, [0, 0, 0, 0, 0, 0, 0, 0]);
        serde_roundtrip!(i64, 1, [1, 0, 0, 0, 0, 0, 0, 0]);
        serde_roundtrip!(i64, 2147483648, [0, 0, 0, 128, 0, 0, 0, 0]);
        serde_roundtrip!(
            i64,
            9223372036854775807,
            [255, 255, 255, 255, 255, 255, 255, 127]
        );

        serde_roundtrip!(
            i128,
            -170141183460469231731687303715884105728,
            [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128]
        );
        serde_roundtrip!(
            i128,
            -170141183460469231731687303715884105727,
            [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128]
        );
        serde_roundtrip!(
            i128,
            -1,
            [
                255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255
            ]
        );
        serde_roundtrip!(i128, 0, [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        serde_roundtrip!(i128, 1, [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        serde_roundtrip!(
            i128,
            9223372036854775808,
            [0, 0, 0, 0, 0, 0, 0, 128, 0, 0, 0, 0, 0, 0, 0, 0]
        );
        serde_roundtrip!(
            i128,
            170141183460469231731687303715884105727,
            [
                255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 127
            ]
        );

        serde_roundtrip!(f32, f32::NEG_INFINITY, [0, 0, 128, 255]);
        serde_roundtrip!(f32, f32::MIN, [255, 255, 127, 255]);
        serde_roundtrip!(f32, -1.0, [0, 0, 128, 191]);
        serde_roundtrip!(f32, -0.0, [0, 0, 0, 128]);
        serde_roundtrip!(f32, 0.0, [0, 0, 0, 0]);
        serde_roundtrip!(f32, f32::MIN_POSITIVE, [0, 0, 128, 0]);
        serde_roundtrip!(f32, f32::EPSILON, [0, 0, 0, 52]);
        serde_roundtrip!(f32, 1.0, [0, 0, 128, 63]);
        serde_roundtrip!(f32, f32::MAX, [255, 255, 127, 127]);
        serde_roundtrip!(f32, f32::INFINITY, [0, 0, 128, 127]);

        serde_roundtrip!(f64, f64::NEG_INFINITY, [0, 0, 0, 0, 0, 0, 240, 255]);
        serde_roundtrip!(f64, f64::MIN, [255, 255, 255, 255, 255, 255, 239, 255]);
        serde_roundtrip!(f64, -1.0, [0, 0, 0, 0, 0, 0, 240, 191]);
        serde_roundtrip!(f64, -0.0, [0, 0, 0, 0, 0, 0, 0, 128]);
        serde_roundtrip!(f64, 0.0, [0, 0, 0, 0, 0, 0, 0, 0]);
        serde_roundtrip!(f64, f64::MIN_POSITIVE, [0, 0, 0, 0, 0, 0, 16, 0]);
        serde_roundtrip!(f64, f64::EPSILON, [0, 0, 0, 0, 0, 0, 176, 60]);
        serde_roundtrip!(f64, 1.0, [0, 0, 0, 0, 0, 0, 240, 63]);
        serde_roundtrip!(f64, f64::MAX, [255, 255, 255, 255, 255, 255, 239, 127]);
        serde_roundtrip!(f64, f64::INFINITY, [0, 0, 0, 0, 0, 0, 240, 127]);

        serde_roundtrip!(bool, false, [0]);
        serde_roundtrip!(bool, true, [1]);
    }

    #[test]
    fn array() {
        serde_roundtrip!([u8; 0], [], []);

        serde_roundtrip!([u8; 1], [0], [0]);
        serde_roundtrip!([u8; 1], [1], [1]);
        serde_roundtrip!([u8; 1], [255], [255]);

        serde_roundtrip!([u8; 2], [0, 0], [0, 0]);
        serde_roundtrip!([u8; 2], [1, 0], [1, 0]);
        serde_roundtrip!([u8; 2], [0, 1], [0, 1]);

        serde_roundtrip!([u8; 3], [0, 0, 0], [0, 0, 0]);
        serde_roundtrip!([u8; 3], [255, 0, 0], [255, 0, 0]);
        serde_roundtrip!([u8; 3], [0, 255, 0], [0, 255, 0]);
        serde_roundtrip!([u8; 3], [0, 0, 255], [0, 0, 255]);
    }

    #[test]
    fn buffer() {
        serde_roundtrip!(Buffer, Buffer::new(&[]), []);

        serde_roundtrip!(Buffer, Buffer::new(&[0]), [0]);
        serde_roundtrip!(Buffer, Buffer::new(&[1]), [1]);
        serde_roundtrip!(Buffer, Buffer::new(&[255]), [255]);

        serde_roundtrip!(Buffer, Buffer::new(&[0, 0]), [0, 0]);
        serde_roundtrip!(Buffer, Buffer::new(&[1, 0]), [1, 0]);
        serde_roundtrip!(Buffer, Buffer::new(&[0, 1]), [0, 1]);

        serde_roundtrip!(Buffer, Buffer::new(&[0, 0, 0]), [0, 0, 0]);
        serde_roundtrip!(Buffer, Buffer::new(&[255, 0, 0]), [255, 0, 0]);
        serde_roundtrip!(Buffer, Buffer::new(&[0, 255, 0]), [0, 255, 0]);
        serde_roundtrip!(Buffer, Buffer::new(&[0, 0, 255]), [0, 0, 255]);
    }

    #[test]
    fn string() {
        serde_roundtrip!(String, String::new(), []);
        serde_roundtrip!(String, "\0".to_owned(), [0]);
        serde_roundtrip!(String, "a".to_owned(), b"a");
        serde_roundtrip!(String, "abcd".to_owned(), b"abcd");
        serde_roundtrip!(String, "💖".to_owned(), [240, 159, 146, 150]);
        serde_roundtrip!(String, "hello".to_owned(), [104, 101, 108, 108, 111]);
    }

    #[test]
    fn estr() {
        serde_roundtrip!(Estr, Estr::from(""), []);
        serde_roundtrip!(Estr, Estr::from("\0"), [0]);
        serde_roundtrip!(Estr, Estr::from("a"), b"a");
        serde_roundtrip!(Estr, Estr::from("abcd"), b"abcd");
        serde_roundtrip!(Estr, Estr::from("💖"), [240, 159, 146, 150]);
        serde_roundtrip!(Estr, Estr::from("hello"), [104, 101, 108, 108, 111]);
    }

    #[test]
    fn smallvec_fixed_size_elements() {
        serde_roundtrip!(SmallVec<[u16; 0]>, SmallVec::new(), []);
        serde_roundtrip!(SmallVec<[u16; 1]>, SmallVec::from_slice(&[0]), [0, 0]);
        serde_roundtrip!(
            SmallVec<[u16; 2]>,
            SmallVec::from_slice(&[255, 256]),
            [255, 0, 0, 1]
        );
        serde_roundtrip!(
            SmallVec<[u16; 3]>,
            SmallVec::from_slice(&[256, 255, 1]),
            [0, 1, 255, 0, 1, 0]
        );
    }

    #[test]
    fn smallvec_variable_size_elements() {
        serde_roundtrip!(SmallVec<[String; 0]>, SmallVec::new(), []);
        serde_roundtrip!(
            SmallVec<[String; 1]>,
            smallvec![String::new()],
            [0, 0, 0, 0]
        );
        serde_roundtrip!(
            SmallVec<[String; 1]>,
            smallvec!["\0".to_owned()],
            [1, 0, 0, 0, 0]
        );
        serde_roundtrip!(
            SmallVec<[String; 1]>,
            smallvec!["a".to_owned()],
            [1, 0, 0, 0, 97]
        );
        serde_roundtrip!(
            SmallVec<[String; 1]>,
            smallvec!["abcd".to_owned()],
            [4, 0, 0, 0, 97, 98, 99, 100]
        );
        serde_roundtrip!(
            SmallVec<[String; 2]>,
            smallvec![String::new(), String::new()],
            [0, 0, 0, 0, 0, 0, 0, 0]
        );
        serde_roundtrip!(
            SmallVec<[String; 2]>,
            smallvec![String::new(), "a".to_owned()],
            [0, 0, 0, 0, 1, 0, 0, 0, 97]
        );
        serde_roundtrip!(
            SmallVec<[String; 2]>,
            smallvec!["a".to_owned(), String::new()],
            [1, 0, 0, 0, 97, 0, 0, 0, 0]
        );
        serde_roundtrip!(
            SmallVec<[String; 2]>,
            smallvec!["a".to_owned(), "b".to_owned()],
            [1, 0, 0, 0, 97, 1, 0, 0, 0, 98]
        );
        serde_roundtrip!(
            SmallVec<[String; 2]>,
            smallvec!["abcd".to_owned(), "dcb".to_owned()],
            [4, 0, 0, 0, 97, 98, 99, 100, 3, 0, 0, 0, 100, 99, 98]
        );
        serde_roundtrip!(
            SmallVec<[String; 3]>,
            smallvec!["a".to_owned(), String::new(), "b".to_owned()],
            [1, 0, 0, 0, 97, 0, 0, 0, 0, 1, 0, 0, 0, 98]
        );
        serde_roundtrip!(
            SmallVec<[SmallVec<[u16; 3]>; 1]>,
            smallvec![smallvec![7, 8, 9]],
            [6, 0, 0, 0, 7, 0, 8, 0, 9, 0]
        );
        serde_roundtrip!(
            SmallVec<[SmallVec<[String; 2]>; 1]>,
            smallvec![smallvec!["".to_owned(), "abcd".to_owned()]],
            [12, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0, 97, 98, 99, 100]
        );
        serde_roundtrip!(
            SmallVec<[SmallVec<[SmallVec<[SmallVec<[u16; 1]>; 2]>; 1]>; 1]>,
            smallvec![smallvec![
                smallvec![smallvec![111], smallvec![222]],
                smallvec![smallvec![256]]
            ]],
            [
                26, 0, 0, 0, 12, 0, 0, 0, 2, 0, 0, 0, 111, 0, 2, 0, 0, 0, 222, 0, 6, 0, 0, 0, 2, 0,
                0, 0, 0, 1
            ]
        );
    }

    #[test]
    fn smallsortedset_fixed_size_elements() {
        serde_roundtrip!(SmallSortedSet<u16, 0>, SmallSortedSet::new(), []);
        serde_roundtrip!(SmallSortedSet<u16, 1>, SmallSortedSet::from_unsorted_slice(&[0]), [0, 0]);
        serde_roundtrip!(
            SmallSortedSet<u16, 2>,
            SmallSortedSet::from_unsorted_slice(&[255, 256]),
            [255, 0, 0, 1]
        );
        serde_roundtrip!(
            SmallSortedSet<u16, 3>,
            SmallSortedSet::from_unsorted_slice(&[256, 255, 1]),
            [1, 0, 255, 0, 0, 1]
        );
    }

    #[test]
    fn smallsortedset_variable_size_elements() {
        serde_roundtrip!(SmallSortedSet<String, 0>, SmallSortedSet::new(), []);
        serde_roundtrip!(
            SmallSortedSet<String, 1>,
            SmallSortedSet::from_unsorted_slice(&[String::new()]),
            [0, 0, 0, 0]
        );
        serde_roundtrip!(
            SmallSortedSet<String, 1>,
            SmallSortedSet::from_unsorted_slice(&["\0".to_owned()]),
            [1, 0, 0, 0, 0]
        );
        serde_roundtrip!(
            SmallSortedSet<String, 1>,
            SmallSortedSet::from_unsorted_slice(&["a".to_owned()]),
            [1, 0, 0, 0, 97]
        );
        serde_roundtrip!(
            SmallSortedSet<String, 1>,
            SmallSortedSet::from_unsorted_slice(&["abcd".to_owned()]),
            [4, 0, 0, 0, 97, 98, 99, 100]
        );
        serde_roundtrip!(
            SmallSortedSet<String, 2>,
            SmallSortedSet::from_unsorted_slice(&[String::new(), String::new()]),
            [0, 0, 0, 0]
        );
        serde_roundtrip!(
            SmallSortedSet<String, 2>,
            SmallSortedSet::from_unsorted_slice(&[String::new(), "a".to_owned()]),
            [0, 0, 0, 0, 1, 0, 0, 0, 97]
        );
        serde_roundtrip!(
            SmallSortedSet<String, 2>,
            SmallSortedSet::from_unsorted_slice(&["a".to_owned(), String::new()]),
            [0, 0, 0, 0, 1, 0, 0, 0, 97]
        );
        serde_roundtrip!(
            SmallSortedSet<String,2>,
            SmallSortedSet::from_unsorted_slice(&["a".to_owned(), "b".to_owned()]),
            [1, 0, 0, 0, 97, 1, 0, 0, 0, 98]
        );
        serde_roundtrip!(
            SmallSortedSet<String, 2>,
            SmallSortedSet::from_unsorted_slice(&["abcd".to_owned(), "dcb".to_owned()]),
            [4, 0, 0, 0, 97, 98, 99, 100, 3, 0, 0, 0, 100, 99, 98]
        );
        serde_roundtrip!(
            SmallSortedSet<String, 3>,
            SmallSortedSet::from_unsorted_slice(&["a".to_owned(), String::new(), "b".to_owned()]),
            [0, 0, 0, 0, 1, 0, 0, 0, 97, 1, 0, 0, 0, 98]
        );
        serde_roundtrip!(
            SmallSortedSet<SmallSortedSet<u16, 3>, 1>,
            SmallSortedSet::from_unsorted_slice(&[SmallSortedSet::from_unsorted_slice(&[7, 8, 9])]),
            [6, 0, 0, 0, 7, 0, 8, 0, 9, 0]
        );
        serde_roundtrip!(
            SmallSortedSet<SmallSortedSet<String, 2>, 1>,
            SmallSortedSet::from_unsorted_slice(&[SmallSortedSet::from_unsorted_slice(&[
                "".to_owned(),
                "abcd".to_owned()
            ])]),
            [12, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0, 97, 98, 99, 100]
        );
        serde_roundtrip!(
            SmallSortedSet<SmallSortedSet<SmallSortedSet<SmallSortedSet<u16, 1>, 2>, 1>, 1>,
            SmallSortedSet::from_unsorted_slice(&[SmallSortedSet::from_unsorted_slice(&[
                SmallSortedSet::from_unsorted_slice(&[
                    SmallSortedSet::from_unsorted_slice(&[111]),
                    SmallSortedSet::from_unsorted_slice(&[222])
                ]),
                SmallSortedSet::from_unsorted_slice(&[SmallSortedSet::from_unsorted_slice(&[256])])
            ])]),
            [
                26, 0, 0, 0, 12, 0, 0, 0, 2, 0, 0, 0, 111, 0, 2, 0, 0, 0, 222, 0, 6, 0, 0, 0, 2, 0,
                0, 0, 0, 1
            ]
        );
    }

    #[test]
    fn tuples() {}
}
