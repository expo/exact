//! Borrowed VM inputs, either a settlement's slice or a field of runtime rows.
use crate::runner::resources::ResourceRow;

/// A read-only column of evaluation inputs. Projection borrows the owning rows.
pub struct Column<'a, T> {
    storage: Storage<'a, T>,
}
enum Storage<'a, T> {
    Slice(&'a [T]),
    Rows(&'a [ResourceRow], fn(&ResourceRow) -> &T),
}
impl<T> Copy for Column<'_, T> {}
impl<T> Clone for Column<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Storage<'_, T> {}
impl<T> Clone for Storage<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<'a, T> Column<'a, T> {
    pub(crate) fn rows(rows: &'a [ResourceRow], field: fn(&ResourceRow) -> &T) -> Self {
        Self {
            storage: Storage::Rows(rows, field),
        }
    }
    /// Number of inputs in this column.
    pub fn len(self) -> usize {
        match self.storage {
            Storage::Slice(s) => s.len(),
            Storage::Rows(s, _) => s.len(),
        }
    }
    /// Whether the column has no inputs.
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }
    /// The input at an index, when present.
    pub fn get(self, i: usize) -> Option<&'a T> {
        match self.storage {
            Storage::Slice(s) => s.get(i),
            Storage::Rows(s, f) => s.get(i).map(f),
        }
    }
    /// Inputs in resource order.
    pub fn iter(self) -> impl ExactSizeIterator<Item = &'a T> {
        (0..self.len()).map(move |i| self.get(i).unwrap())
    }
    /// An owned snapshot of these inputs.
    pub fn to_vec(self) -> Vec<T>
    where
        T: Clone,
    {
        self.iter().cloned().collect()
    }
}
impl<'a, T> From<&'a [T]> for Column<'a, T> {
    fn from(s: &'a [T]) -> Self {
        Self {
            storage: Storage::Slice(s),
        }
    }
}
impl<'a, T> From<&'a Vec<T>> for Column<'a, T> {
    fn from(s: &'a Vec<T>) -> Self {
        s.as_slice().into()
    }
}
impl<'a, T, const N: usize> From<&'a [T; N]> for Column<'a, T> {
    fn from(s: &'a [T; N]) -> Self {
        s.as_slice().into()
    }
}
impl<T> std::ops::Index<usize> for Column<'_, T> {
    type Output = T;
    fn index(&self, i: usize) -> &T {
        self.get(i).unwrap()
    }
}

/// Iterates a borrowed column without allocating.
pub struct Iter<'a, T> {
    column: Column<'a, T>,
    at: usize,
}
impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<Self::Item> {
        let value = self.column.get(self.at)?;
        self.at += 1;
        Some(value)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.column.len() - self.at;
        (n, Some(n))
    }
}
impl<T> ExactSizeIterator for Iter<'_, T> {}
impl<'a, T> IntoIterator for Column<'a, T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        Iter {
            column: self,
            at: 0,
        }
    }
}
