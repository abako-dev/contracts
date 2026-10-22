//! Private v5 storage cells remain below Kreivo's unchanged 16 KiB value limit.
//! The VM still meters all reads/writes, memory and decoding. This is not an
//! unbounded archive reader or an alternative runtime storage policy.
#![cfg_attr(not(feature = "std"), no_std)]
use core::marker::PhantomData;
use ink::prelude::vec::Vec;
use scale::{Decode, DecodeAll, Encode, EncodeLike};
pub const CHUNK_BYTES: usize = 8 * 1024;
const ROOT: u32 = u32::MAX;
fn count(bytes: usize) -> usize {
    bytes.div_ceil(CHUNK_BYTES)
}
fn read<K: Encode, V: Encode + Decode>(key: &K) -> Option<V> {
    ink::env::get_contract_storage(key).unwrap_or_else(|_| panic!("invalid paged storage"))
}
fn write<K: Encode, V: Encode + Decode>(key: &K, value: &V) {
    ink::env::set_contract_storage(key, value);
}
fn clear<K: Encode>(key: &K) {
    ink::env::clear_contract_storage(key);
}
fn decode<T: DecodeAll>(bytes: &[u8]) -> T {
    T::decode_all(&mut &bytes[..]).unwrap_or_else(|_| panic!("invalid paged value"))
}
/// Root state pages; the ink storage header retains only its encoded byte length.
pub fn load<T: DecodeAll>(length: u32) -> T {
    let mut bytes = Vec::with_capacity(length as usize);
    for index in 0..count(length as usize) {
        let chunk: Vec<u8> =
            read(&(ROOT, index as u32)).unwrap_or_else(|| panic!("missing root page"));
        bytes.extend(chunk);
    }
    assert_eq!(bytes.len(), length as usize, "invalid root length");
    decode(&bytes)
}
/// Write successful tentative state and remove pages left after shrinking it.
pub fn save<T: Encode>(old_length: u32, value: &T) -> u32 {
    let bytes = value.encode();
    let length = u32::try_from(bytes.len()).unwrap_or_else(|_| panic!("root too large"));
    for (index, chunk) in bytes.chunks(CHUNK_BYTES).enumerate() {
        write(&(ROOT, index as u32), &chunk.to_vec());
    }
    for index in count(bytes.len())..count(old_length as usize) {
        clear(&(ROOT, index as u32));
    }
    length
}
/// Read one immutable page without loading the aggregate into contract memory.
pub fn page(length: u32, index: u32) -> Option<Vec<u8>> {
    if index as usize >= count(length as usize) {
        return None;
    }
    read(&(ROOT, index))
}
/// Explicit private namespaces keep each mapping value in independent small cells.
/// Encoding contains no runtime handles; only the owning contract can access cells.
#[derive(Debug, Encode, Decode)]
pub struct PagedMapping<K, V, const PREFIX: u32>(PhantomData<(K, V)>);
impl<K, V, const PREFIX: u32> Default for PagedMapping<K, V, PREFIX> {
    fn default() -> Self {
        Self(PhantomData)
    }
}
impl<K: Encode, V: Encode + DecodeAll, const PREFIX: u32> PagedMapping<K, V, PREFIX> {
    pub fn get<Q: EncodeLike<K> + Clone>(&self, key: Q) -> Option<V> {
        let length: u32 = read(&(PREFIX, &key, 0u32))?;
        let mut bytes = Vec::with_capacity(length as usize);
        for index in 0..count(length as usize) {
            let chunk: Vec<u8> = read(&(PREFIX, &key, index as u32 + 1))
                .unwrap_or_else(|| panic!("missing value page"));
            bytes.extend(chunk);
        }
        assert_eq!(bytes.len(), length as usize, "invalid value length");
        Some(decode(&bytes))
    }
    pub fn contains<Q: EncodeLike<K>>(&self, key: Q) -> bool {
        ink::env::contains_contract_storage(&(PREFIX, key, 0u32)).is_some()
    }
    pub fn insert<Q: EncodeLike<K>, R: EncodeLike<V> + Encode>(&mut self, key: Q, value: &R) {
        let bytes = value.encode();
        let length = u32::try_from(bytes.len()).unwrap_or_else(|_| panic!("value too large"));
        let old_length: u32 = read(&(PREFIX, &key, 0u32)).unwrap_or(0);
        for (index, chunk) in bytes.chunks(CHUNK_BYTES).enumerate() {
            write(&(PREFIX, &key, index as u32 + 1), &chunk.to_vec());
        }
        for index in count(bytes.len())..count(old_length as usize) {
            clear(&(PREFIX, &key, index as u32 + 1));
        }
        write(&(PREFIX, key, 0u32), &length);
    }
    pub fn remove<Q: EncodeLike<K>>(&mut self, key: Q) {
        if let Some(length) = read::<_, u32>(&(PREFIX, &key, 0u32)) {
            for index in 0..count(length as usize) {
                clear(&(PREFIX, &key, index as u32 + 1));
            }
            clear(&(PREFIX, key, 0u32));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[ink::test]
    fn pages_preserve_values_and_remove_shrunken_state() {
        let large = ink::prelude::vec![7u8; 40_000];
        let old = save(0, &large);
        assert_eq!(load::<Vec<u8>>(old), large);
        for index in 0..count(old as usize) {
            let bytes = page(old, index as u32).unwrap();
            assert!(bytes.len() <= CHUNK_BYTES);
        }
        let short = ink::prelude::vec![8u8; 9];
        let new = save(old, &short);
        assert_eq!(load::<Vec<u8>>(new), short);
        assert!(page(old, 1).is_none());
    }
    #[ink::test]
    fn mapping_namespaces_remain_separate_and_deleted_pages_disappear() {
        let key = [3u8; 32];
        let mut left = PagedMapping::<[u8; 32], Vec<u8>, 1>::default();
        let mut right = PagedMapping::<[u8; 32], Vec<u8>, 2>::default();
        let large = ink::prelude::vec![9u8;40_000];
        left.insert(key, &large);
        right.insert(key, &ink::prelude::vec![4u8]);
        assert_eq!(left.get(key), Some(large));
        assert_eq!(right.get(key), Some(ink::prelude::vec![4u8]));
        let size = ink::env::contains_contract_storage(&(1u32, key, 1u32)).unwrap();
        assert!(size < 16 * 1024);
        left.insert(key, &ink::prelude::vec![2u8]);
        assert!(ink::env::contains_contract_storage(&(1u32, key, 2u32)).is_none());
        left.remove(key);
        assert!(!left.contains(key));
        assert_eq!(left.get(key), None);
        assert!(ink::env::contains_contract_storage(&(1u32, key, 1u32)).is_none());
        assert!(right.contains(key));
    }
}
