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
