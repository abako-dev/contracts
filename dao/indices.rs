//! Ordered fixed-size indexes using ink's standard per-element storage.
use ink::{
    prelude::vec::Vec,
    storage::{
        traits::{Packed, StorageKey},
        StorageVec,
    },
};

pub(crate) fn values<T: Packed, K: StorageKey>(items: &StorageVec<T, K>) -> Vec<T> {
    (0..items.len())
        .map(|i| items.get(i).expect("missing index entry"))
        .collect()
}
fn position<T: Packed + Ord + scale::EncodeLike, K: StorageKey>(
    items: &StorageVec<T, K>,
    value: &T,
) -> core::result::Result<u32, u32> {
    let (mut low, mut high) = (0, items.len());
    while low < high {
        let mid = low.midpoint(high);
        match items.get(mid).expect("missing index entry").cmp(value) {
            core::cmp::Ordering::Less => low = mid.checked_add(1).expect("index overflow"),
            core::cmp::Ordering::Greater => high = mid,
            core::cmp::Ordering::Equal => return Ok(mid),
        }
    }
    Err(low)
}
pub(crate) fn insert<T: Packed + Ord + scale::EncodeLike, K: StorageKey>(
    items: &mut StorageVec<T, K>,
    value: T,
) {
    let Err(position) = position(items, &value) else {
        return;
    };
    let length = items.len();
    // Standard ink operations trap on a storage failure; the outer message must
    // revert the entire mutation, including earlier index and business writes.
    items.push(&value);
    for i in (position..length).rev() {
        let previous = items.get(i).expect("missing index entry");
        items.set(i.checked_add(1).expect("index overflow"), &previous);
    }
    items.set(position, &value);
}
pub(crate) fn remove<T: Packed + Ord + scale::EncodeLike, K: StorageKey>(
    items: &mut StorageVec<T, K>,
    value: &T,
) {
    let Ok(position) = position(items, value) else {
        return;
    };
    let last = items.len().checked_sub(1).expect("missing index tail");
    for i in position..last {
        let next = items
            .get(i.checked_add(1).expect("index overflow"))
            .expect("missing index entry");
        items.set(i, &next);
    }
    items.pop().expect("missing index tail");
}

/// What a message has read under each key, to read a cell once. A sorted
/// vector, because every map type a contract uses adds to its code, and the
/// code is charged to every call.
pub(crate) struct Seen<K, V>(Vec<(K, V)>);
impl<K, V> Default for Seen<K, V> {
    fn default() -> Self {
        Self(Vec::new())
    }
}
impl<K: Ord + Copy, V> Seen<K, V> {
    fn position(&self, key: &K) -> core::result::Result<usize, usize> {
        self.0.binary_search_by(|(seen, _)| seen.cmp(key))
    }
    pub(crate) fn get(&self, key: &K) -> Option<&V> {
        self.position(key).ok().map(|index| &self.0[index].1)
    }
    pub(crate) fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.position(key).ok().map(|index| &mut self.0[index].1)
    }
    /// The value seen for `key`, reading it with `read` the first time.
    pub(crate) fn get_or(&mut self, key: K, read: impl FnOnce() -> V) -> V
    where
        V: Copy,
    {
        match self.position(&key) {
            Ok(index) => self.0[index].1,
            Err(index) => {
                let value = read();
                self.0.insert(index, (key, value));
                value
            }
        }
    }
    pub(crate) fn insert(&mut self, key: K, value: V) {
        match self.position(&key) {
            Ok(index) => self.0[index].1 = value,
            Err(index) => self.0.insert(index, (key, value)),
        }
    }
    pub(crate) fn into_values(self) -> impl Iterator<Item = (K, V)> {
        self.0.into_iter()
    }
}
