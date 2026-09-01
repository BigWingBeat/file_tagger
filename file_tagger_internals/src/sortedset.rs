use std::{
    borrow::Borrow,
    hash::{Hash, Hasher},
    ops::{Deref, Index, RangeBounds},
    slice::SliceIndex,
};

/// A bit like a `BTreeSet`, but backed by a single sorted `Vec<T>` instead of a tree of nodes.
/// This makes it simpler, and faster to construct and read from, in exchange for mutations being slower.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct SortedSet<T> {
    vec: Vec<T>,
}

impl<T> SortedSet<T> {
    #[inline]
    pub const fn new() -> Self {
        Self { vec: Vec::new() }
    }

    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            vec: Vec::with_capacity(capacity),
        }
    }

    /// Consume `self` and return the sorted inner `Vec`
    #[inline]
    pub fn into_vec(self) -> Vec<T> {
        self.vec
    }

    /// See [`Vec::reserve`]
    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        self.vec.reserve(additional);
    }

    /// See [`Vec::reserve_exact`]
    #[inline]
    pub fn reserve_exact(&mut self, additional: usize) {
        self.vec.reserve_exact(additional);
    }

    /// See [`Vec::shrink_to_fit`]
    #[inline]
    pub fn shrink_to_fit(&mut self) {
        self.vec.shrink_to_fit();
    }

    /// See [`Vec::shrink_to`]
    #[inline]
    pub fn shrink_to(&mut self, min_capacity: usize) {
        self.vec.shrink_to(min_capacity);
    }

    // We have an inherent method for this, rather than forwarding to [`Slice::len`] via `Deref`, because `Vec` does too I guess
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
    /// See: [`Vec::drain`]
    #[inline]
    pub fn drain(&mut self, range: impl RangeBounds<usize>) -> std::vec::Drain<'_, T> {
        self.vec.drain(range)
    }

    /// Retains only the elements for which `F` returns `true`, removing all other elements.
    /// See: [`Vec::retain`]
    #[inline]
    pub fn retain(&mut self, f: impl FnMut(&T) -> bool) {
        self.vec.retain(f)
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

    /// Directly inserts the given element at the given index, without checking for correct sort order.
    /// Returns a reference to the new element.
    ///
    /// # Safety
    /// The collection must still be sorted after the given element is inserted at the given index.
    ///
    /// # Panic
    /// Panics if the given index is out of bounds.
    #[must_use = "if you don't need a reference to the value, use `SortedSet::insert_at` instead"]
    #[inline]
    pub unsafe fn insert_at_mut(&mut self, index: usize, element: T) -> &mut T {
        self.vec.insert_mut(index, element)
    }
}

impl<T: Ord> SortedSet<T> {
    /// Sorts and deduplicates the given elements
    pub fn from_unsorted(mut vec: Vec<T>) -> Self {
        vec.sort_unstable();
        vec.dedup();
        Self { vec }
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

impl<T: Ord + Clone> SortedSet<T> {
    /// Clones, sorts, and deduplicates the given elements
    pub fn from_unsorted_slice(slice: &[T]) -> Self {
        let mut vec = slice.to_vec();
        vec.sort_unstable();
        vec.dedup();
        Self { vec }
    }
}

impl<T> Default for SortedSet<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Ord> From<Vec<T>> for SortedSet<T> {
    #[inline]
    fn from(unsorted: Vec<T>) -> Self {
        Self::from_unsorted(unsorted)
    }
}

impl<T: Ord + Clone> From<&[T]> for SortedSet<T> {
    #[inline]
    fn from(unsorted: &[T]) -> Self {
        Self::from_unsorted_slice(unsorted)
    }
}

// Like with std `Vec`, this provides a lot of useful methods that we would otherwise have to impl ourselves.
// Note that unlike std `Vec`, we do not also impl `DerefMut`, as that would allow violating both of our invariants.
impl<T> Deref for SortedSet<T> {
    type Target = [T];

    #[inline]
    fn deref(&self) -> &[T] {
        &self.vec
    }
}

// Ditto for not also implementing `AsMut`
impl<T> AsRef<[T]> for SortedSet<T> {
    #[inline]
    fn as_ref(&self) -> &[T] {
        &self.vec
    }
}

// Ditto ditto for not also implementing `BorrowMut`
impl<T> Borrow<[T]> for SortedSet<T> {
    #[inline]
    fn borrow(&self) -> &[T] {
        &self.vec
    }
}

// Ditto ditto ditto for not also implementing `IndexMut`
impl<T, I: SliceIndex<[T]>> Index<I> for SortedSet<T> {
    type Output = I::Output;

    #[inline]
    fn index(&self, index: I) -> &Self::Output {
        self.vec.index(index)
    }
}

impl<A: Ord> Extend<A> for SortedSet<A> {
    fn extend<T: IntoIterator<Item = A>>(&mut self, iter: T) {
        self.vec.extend(iter);
        // Prefer stable sort to unstable sort here as we know the extended vec is already sorted up to the new elements
        self.vec.sort();
        self.vec.dedup();
    }
}

impl<A: Ord> FromIterator<A> for SortedSet<A> {
    fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
        let vec = Vec::from_iter(iter);
        Self::from_unsorted(vec)
    }
}

impl<T> IntoIterator for SortedSet<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.vec.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a SortedSet<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.vec.iter()
    }
}

impl<T: Hash> Hash for SortedSet<T> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.vec.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorted_set() {
        let mut s = SortedSet::new();
        assert_eq!(s.insert(5), Ok(0));
        assert_eq!(s.insert(3), Ok(0));
        assert_eq!(s.insert(4), Ok(1));
        assert_eq!(s.insert(4), Err(1));
        assert_eq!(s.len(), 3);
        assert_eq!(s.binary_search(&3), Ok(0));
        assert_eq!(
            *SortedSet::from_unsorted(vec![5, -10, 99, -10, -11, 10, 2, 17, 10]),
            vec![-11, -10, 2, 5, 10, 17, 99]
        );
        assert_eq!(
            SortedSet::from_unsorted(vec![5, -10, 99, -10, -11, 10, 2, 17, 10]),
            vec![5, -10, 99, -10, -11, 10, 2, 17, 10].into()
        );
        let mut s = SortedSet::new();
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
