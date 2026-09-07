use std::{convert::Infallible, io::Write, ops::DerefMut, str::Utf8Error};

use byteview::ByteView;
use estr::Estr;
use thiserror::Error;

use super::{
    AsBytes, Buffer, Bytes, FromBytes, Prefixable, Reader, SizeHint, SmallSortedSet, SmallVec,
    UnexpectedEof,
};

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

            // `as_bytes` is only expensive if it does a heap allocation, which is rare (mostly just collection types do).
            // Most impls are cheap reference-to-reference conversions, or just write out fixed-size arrays on the stack.
            // So, serializing all of our elements twice is most likely not that costly.
            let total_bytes: usize = self.iter().map(|element| element.as_bytes().len()).sum();
            let total_bytes = total_bytes + total_length_prefix_bytes;
            let mut bytes = ByteView::builder(total_bytes);

            // The remainder of un-written bytes, that will shrink as elements are written, while `bytes` retains the whole thing
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

#[derive(Error)]
pub enum SmallVecError<T: FromBytes> {
    #[error("unexpected EOF while reading length prefix")]
    LengthPrefixEof(#[source] UnexpectedEof),
    #[error("unexpected EOF while deserializing list element")]
    ElementEof(#[source] UnexpectedEof),
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
            Self::LengthPrefixEof(eof) => f.debug_tuple("LengthPrefixEof").field(eof).finish(),
            Self::ElementEof(eof) => f.debug_tuple("ElementEof").field(eof).finish(),
            Self::Deser(e) => f.debug_tuple("Deser").field(e).finish(),
        }
    }
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

/* SmallSortedSet */

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

/// You know...
impl<T: FromBytes, U: FromBytes> std::fmt::Debug for TupleError<T, U>
where
    T::Error: std::fmt::Debug,
    U::Error: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LengthPrefixEof(eof) => f.debug_tuple("LengthPrefixEof").field(eof).finish(),
            Self::FirstEof(eof) => f.debug_tuple("FirstEof").field(eof).finish(),
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
