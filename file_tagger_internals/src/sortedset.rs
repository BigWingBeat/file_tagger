use std::{
    borrow::Borrow,
    hash::{Hash, Hasher},
    ops::{Deref, Index, RangeBounds},
    slice::SliceIndex,
};

use smallvec::SmallVec;

/// A collection that guarantees its elements are always in sorted order, and that there are no duplicate elements.
/// Is stored inline on the stack for up to `N` elements, beyond which it automatically spills over to a heap allocation.
///
/// A bit like a `BTreeSet`, but backed by a single sorted `SmallVec<[T; N]>` instead of a tree of nodes.
/// This makes it simpler, and faster to construct and read from, in exchange for mutations being slower.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct SmallSortedSet<T, const N: usize> {
    vec: SmallVec<[T; N]>,
}

impl<T, const N: usize> SmallSortedSet<T, N> {
    #[inline]
    pub const fn new() -> Self {
        Self {
            vec: SmallVec::new_const(),
        }
    }

    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            vec: SmallVec::with_capacity(capacity),
        }
    }

    /// Returns a reference to the inner `SmallVec`
    #[inline]
    pub fn as_vec(&self) -> &SmallVec<[T; N]> {
        &self.vec
    }

    /// Consume `self` and return the sorted inner `SmallVec`
    #[inline]
    pub fn into_vec(self) -> SmallVec<[T; N]> {
        self.vec
    }

    /// See [`SmallVec::reserve`]
    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        self.vec.reserve(additional);
    }

    /// See [`SmallVec::reserve_exact`]
    #[inline]
    pub fn reserve_exact(&mut self, additional: usize) {
        self.vec.reserve_exact(additional);
    }

    /// See [`SmallVec::shrink_to_fit`]
    #[inline]
    pub fn shrink_to_fit(&mut self) {
        self.vec.shrink_to_fit();
    }

    /// Returns the number of items the collection can hold without reallocating.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.vec.capacity()
    }

    // We have an inherent method for this, rather than forwarding to [`Slice::len`] via `Deref`, because `SmallVec` does too I guess
    /// Returns the number of elements in the collection.
    #[inline]
    pub fn len(&self) -> usize {
        self.vec.len()
    }

    /// Removes and returns the element at the given index.
    ///
    /// # Panic
    /// Panics if `index` is out of bounds.
    #[inline]
    pub fn remove_at(&mut self, index: usize) -> T {
        self.vec.remove(index)
    }

    /// Removes and returns the element at the given index. If the given index is out of bounds, returns `None`.
    #[inline]
    pub fn try_remove_at(&mut self, index: usize) -> Option<T> {
        if index >= self.vec.len() {
            return None;
        }
        Some(self.vec.remove(index))
    }

    /// Removes and returns the last element, or `None` if the collection is empty.
    #[inline]
    pub fn pop(&mut self) -> Option<T> {
        self.vec.pop()
    }

    /// Clears the collection, removing all values.
    #[inline]
    pub fn clear(&mut self) {
        self.vec.clear()
    }

    /// Shrinks the collection to `len` elements, dropping everything at and after that index.
    /// If the given length is greater than or equal to the current number of elements, this does nothing.
    #[inline]
    pub fn truncate(&mut self, len: usize) {
        self.vec.truncate(len);
    }

    /// Removes a range of elements, returning a double-ended iterator over the removed subslice.
    /// See: [`SmallVec::drain`]
    #[inline]
    pub fn drain(&mut self, range: impl RangeBounds<usize>) -> smallvec::Drain<'_, [T; N]> {
        self.vec.drain(range)
    }

    /// Retains only the elements for which `F` returns `true`, removing all other elements.
    /// See: [`SmallVec::retain`]
    #[inline]
    pub fn retain(&mut self, mut f: impl FnMut(&T) -> bool) {
        // For some stupid reason `SmallVec::retain` passes `&mut T` instead of `&T` despite also having a separate `retain_mut` method
        self.vec.retain(|t| f(t))
    }

    /// Directly inserts the given element at the given index, without checking for correct sort order.
    ///
    /// # Safety
    /// The collection must still be sorted after the given element is inserted at the given index.
    ///
    /// # Panic
    /// Panics if the given index is out of bounds.
    #[inline]
    pub unsafe fn insert_at(&mut self, index: usize, element: T) {
        self.vec.insert(index, element)
    }

    // Waiting on SmallVec to update to add this method
    // /// Directly inserts the given element at the given index, without checking for correct sort order.
    // /// Returns a reference to the new element.
    // ///
    // /// # Safety
    // /// The collection must still be sorted after the given element is inserted at the given index.
    // ///
    // /// # Panic
    // /// Panics if the given index is out of bounds.
    // #[must_use = "if you don't need a reference to the value, use `SortedSet::insert_at` instead"]
    // #[inline]
    // pub unsafe fn insert_at_mut(&mut self, index: usize, element: T) -> &mut T {
    //     self.vec.insert_mut(index, element)
    // }
}

impl<T: Ord, const N: usize> SmallSortedSet<T, N> {
    /// Sorts and deduplicates the given elements
    pub fn from_unsorted(mut vec: SmallVec<[T; N]>) -> Self {
        vec.sort_unstable();
        vec.dedup();
        Self { vec }
    }

    /// Sorts and deduplicates the given elements
    #[inline]
    pub fn from_unsorted_vec(vec: Vec<T>) -> Self {
        Self::from_unsorted(vec.into())
    }

    // We have an inherent method for this, rather than forwarding to [`Slice::contains`] via `Deref`, to avoid the linear scan
    /// Returns `true` if the given element is present in the collection.
    #[inline]
    pub fn contains(&self, element: &T) -> bool {
        self.vec.binary_search(element).is_ok()
    }

    /// Inserts the given element into sorted position.
    /// This returns `Ok` if the element was successfully inserted, and returns `Err`
    /// if the element was already present, in both cases carrying the index of the element.
    pub fn insert(&mut self, element: T) -> Result<usize, usize> {
        // The return value of `binary_search` is the opposite of what we want, as it returns `Ok` if its already present etc.
        match self.vec.binary_search(&element) {
            Ok(i) => Err(i),
            Err(i) => {
                self.vec.insert(i, element);
                Ok(i)
            }
        }
    }

    /// Removes the given element, if it is present.
    /// Returns `Ok` if the element was successfully removed, with the (former) index of the element.
    /// If the element was not present, returns `Err` and the index that the element would have been at.
    pub fn remove(&mut self, element: &T) -> Result<usize, usize> {
        let result = self.vec.binary_search(element);
        if let Ok(i) = result {
            self.vec.remove(i);
        }
        result
    }
}

impl<T: Ord + Clone, const N: usize> SmallSortedSet<T, N> {
    /// Clones, sorts, and deduplicates the given elements
    #[inline]
    pub fn from_unsorted_slice(slice: &[T]) -> Self {
        Self::from_unsorted(slice.into())
    }
}

impl<T, const N: usize> Default for SmallSortedSet<T, N> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Ord, const N: usize> From<SmallVec<[T; N]>> for SmallSortedSet<T, N> {
    #[inline]
    fn from(unsorted: SmallVec<[T; N]>) -> Self {
        Self::from_unsorted(unsorted)
    }
}

impl<T: Ord, const N: usize> From<Vec<T>> for SmallSortedSet<T, N> {
    #[inline]
    fn from(unsorted: Vec<T>) -> Self {
        Self::from_unsorted_vec(unsorted)
    }
}

impl<T: Ord + Clone, const N: usize> From<&[T]> for SmallSortedSet<T, N> {
    #[inline]
    fn from(unsorted: &[T]) -> Self {
        Self::from_unsorted_slice(unsorted)
    }
}

// Like with `SmallVec`, this provides a lot of useful methods that we would otherwise have to impl ourselves.
// Note that unlike `SmallVec`, we do not also impl `DerefMut`, as that would allow violating both of our invariants.
impl<T, const N: usize> Deref for SmallSortedSet<T, N> {
    type Target = [T];

    #[inline]
    fn deref(&self) -> &[T] {
        &self.vec
    }
}

// Ditto for not also implementing `AsMut`
impl<T, const N: usize> AsRef<[T]> for SmallSortedSet<T, N> {
    #[inline]
    fn as_ref(&self) -> &[T] {
        &self.vec
    }
}

impl<T, const N: usize> AsRef<SmallVec<[T; N]>> for SmallSortedSet<T, N> {
    #[inline]
    fn as_ref(&self) -> &SmallVec<[T; N]> {
        &self.vec
    }
}

// Ditto ditto for not also implementing `BorrowMut`
impl<T, const N: usize> Borrow<[T]> for SmallSortedSet<T, N> {
    #[inline]
    fn borrow(&self) -> &[T] {
        &self.vec
    }
}

// Ditto ditto ditto for not also implementing `IndexMut`
impl<T, I: SliceIndex<[T]>, const N: usize> Index<I> for SmallSortedSet<T, N> {
    type Output = I::Output;

    #[inline]
    fn index(&self, index: I) -> &Self::Output {
        self.vec.index(index)
    }
}

impl<A: Ord, const N: usize> Extend<A> for SmallSortedSet<A, N> {
    fn extend<T: IntoIterator<Item = A>>(&mut self, iter: T) {
        self.vec.extend(iter);
        // Prefer stable sort to unstable sort here as we know the extended vec is already sorted up to the new elements
        self.vec.sort();
        self.vec.dedup();
    }
}

impl<A: Ord, const N: usize> FromIterator<A> for SmallSortedSet<A, N> {
    fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
        let vec = SmallVec::from_iter(iter);
        Self::from_unsorted(vec)
    }
}

impl<T, const N: usize> IntoIterator for SmallSortedSet<T, N> {
    type Item = T;
    type IntoIter = smallvec::IntoIter<[T; N]>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.vec.into_iter()
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a SmallSortedSet<T, N> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.vec.iter()
    }
}

impl<T: Hash, const N: usize> Hash for SmallSortedSet<T, N> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.vec.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use smallvec::smallvec_inline;

    use super::*;

    #[test]
    fn sorted_set() {
        let mut s = SmallSortedSet::<_, 3>::new();
        assert_eq!(s.insert(5), Ok(0));
        assert_eq!(s.insert(3), Ok(0));
        assert_eq!(s.insert(4), Ok(1));
        assert_eq!(s.insert(4), Err(1));
        assert_eq!(s.len(), 3);
        assert_eq!(s.binary_search(&3), Ok(0));

        assert_eq!(
            *SmallSortedSet::from_unsorted(smallvec_inline![5, -10, 99, -10, -11, 10, 2, 17, 10]),
            vec![-11, -10, 2, 5, 10, 17, 99]
        );

        assert_eq!(
            SmallSortedSet::from_unsorted(smallvec_inline![5, -10, 99, -10, -11, 10, 2, 17, 10]),
            vec![5, -10, 99, -10, -11, 10, 2, 17, 10].into()
        );

        let mut s = SmallSortedSet::<_, 7>::new();
        s.extend([5, -11, -10, 99, -11, 2, 17, 2, 10]);
        assert_eq!(*s, vec![-11, -10, 2, 5, 10, 17, 99]);
        s.remove_at(0);
        let _ = s.insert(1);
        assert_eq!(
            s.drain(..).collect::<Vec<i32>>(),
            vec![-10, 1, 2, 5, 10, 17, 99]
        );
    }
}
