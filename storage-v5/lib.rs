//! Private v5 storage cells remain below Kreivo's unchanged 16 KiB value limit.
//! Stream SCALE through fixed pages so a valid large payload does not require
//! several full copies in the contract's bounded WASM memory.
#![cfg_attr(not(feature = "std"), no_std)]
use core::marker::PhantomData;
use ink::{prelude::vec::Vec, storage::traits::Storable};
use scale::{Decode, Encode, EncodeLike, Input, Output};
pub const CHUNK_BYTES: usize = 8 * 1024;
const ROOT: u32 = u32::MAX;
fn count(bytes: usize) -> usize {
    bytes.div_ceil(CHUNK_BYTES)
}
fn read<K: Encode, V: Storable>(key: &K) -> Option<V> {
    ink::env::get_contract_storage(key).unwrap_or_else(|_| panic!("invalid paged storage"))
}
fn write<K: Encode, V: Storable>(key: &K, value: &V) {
    ink::env::set_contract_storage(key, value);
}
fn clear<K: Encode>(key: &K) {
    ink::env::clear_contract_storage(key);
}
// Borrow the encoder's reusable page; allocating a Vec per cell exhausts ink's
// bump allocator even when each individual allocation is small.
struct Bytes<'a>(&'a [u8]);
impl Storable for Bytes<'_> {
    fn encode<T: Output + ?Sized>(&self, dest: &mut T) {
        self.0.encode_to(dest);
    }
    fn decode<I: Input>(_: &mut I) -> Result<Self, scale::Error> {
        Err("write-only page".into())
    }
    fn encoded_size(&self) -> usize {
        Encode::encoded_size(&self.0)
    }
}
struct Block {
    bytes: [u8; CHUNK_BYTES],
    length: usize,
}
impl Storable for Block {
    fn encode<T: Output + ?Sized>(&self, dest: &mut T) {
        self.bytes[..self.length].encode_to(dest);
    }
    fn decode<I: Input>(input: &mut I) -> Result<Self, scale::Error> {
        let length = <scale::Compact<u32> as Decode>::decode(input)?.0 as usize;
        if length > CHUNK_BYTES {
            return Err("oversized page".into());
        }
        let mut page = Self {
            bytes: [0; CHUNK_BYTES],
            length,
        };
        input.read(&mut page.bytes[..length])?;
        Ok(page)
    }
    fn encoded_size(&self) -> usize {
        Encode::encoded_size(&self.bytes[..self.length])
    }
}
struct Pages<F> {
    next: F,
    length: usize,
    position: usize,
    index: u32,
    page: Block,
    offset: usize,
}
impl<F: FnMut(u32) -> Option<Block>> Input for Pages<F> {
    fn remaining_len(&mut self) -> Result<Option<usize>, scale::Error> {
        Ok(Some(self.length - self.position))
    }
    fn read(&mut self, mut into: &mut [u8]) -> Result<(), scale::Error> {
        if into.len() > self.length - self.position {
            return Err("truncated paged value".into());
        }
        while !into.is_empty() {
            if self.offset == self.page.length {
                let page = (self.next)(self.index).ok_or("missing page")?;
                let expected = core::cmp::min(CHUNK_BYTES, self.length - self.position);
                if page.length != expected {
                    return Err("invalid page length".into());
                }
                self.page = page;
                self.offset = 0;
                self.index += 1;
            }
            let copied = core::cmp::min(into.len(), self.page.length - self.offset);
            into[..copied].copy_from_slice(&self.page.bytes[self.offset..self.offset + copied]);
            self.position += copied;
            self.offset += copied;
            into = &mut into[copied..];
        }
        Ok(())
    }
}
fn decode<T: Decode>(length: u32, next: impl FnMut(u32) -> Option<Block>) -> T {
    let mut pages = Pages {
        next,
        length: length as usize,
        position: 0,
        index: 0,
        page: Block {
            bytes: [0; CHUNK_BYTES],
            length: 0,
        },
        offset: 0,
    };
    let value = T::decode(&mut pages).unwrap_or_else(|_| panic!("invalid paged value"));
    assert_eq!(pages.position, pages.length, "trailing paged bytes");
    value
}
struct Writer<F> {
    flush: F,
    page: [u8; CHUNK_BYTES],
    used: usize,
    index: u32,
    length: usize,
}
impl<F: FnMut(u32, &[u8])> Writer<F> {
    fn flush(&mut self) {
        if self.used != 0 {
            (self.flush)(self.index, &self.page[..self.used]);
            self.index += 1;
            self.used = 0;
        }
    }
}
impl<F: FnMut(u32, &[u8])> Output for Writer<F> {
    fn write(&mut self, mut bytes: &[u8]) {
        self.length += bytes.len();
        while !bytes.is_empty() {
            let copied = core::cmp::min(bytes.len(), CHUNK_BYTES - self.used);
            self.page[self.used..self.used + copied].copy_from_slice(&bytes[..copied]);
            self.used += copied;
            bytes = &bytes[copied..];
            if self.used == CHUNK_BYTES {
                self.flush();
            }
        }
    }
}
fn encode<T: Encode>(value: &T, flush: impl FnMut(u32, &[u8])) -> u32 {
    let length = u32::try_from(value.encoded_size()).unwrap_or_else(|_| panic!("value too large"));
    let mut writer = Writer {
        flush,
        page: [0; CHUNK_BYTES],
        used: 0,
        index: 0,
        length: 0,
    };
    value.encode_to(&mut writer);
    writer.flush();
    assert_eq!(writer.length, length as usize, "inconsistent encoding");
    length
}
/// Root state pages; the ink storage header retains only encoded byte length.
pub fn load<T: Decode>(length: u32) -> T {
    decode(length, |index| read(&(ROOT, index)))
}
/// Save successful tentative state and clear cells left after shrinking it.
pub fn save<T: Encode>(old_length: u32, value: &T) -> u32 {
    let length = encode(value, |index, bytes| write(&(ROOT, index), &Bytes(bytes)));
    for index in count(length as usize)..count(old_length as usize) {
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
/// Separate fixed namespaces keep values in independently addressed small cells.
#[derive(Debug, Encode, Decode)]
pub struct PagedMapping<K, V, const PREFIX: u32>(PhantomData<(K, V)>);
impl<K, V, const PREFIX: u32> Default for PagedMapping<K, V, PREFIX> {
    fn default() -> Self {
        Self(PhantomData)
    }
}
impl<K: Encode, V: Encode + Decode, const PREFIX: u32> PagedMapping<K, V, PREFIX> {
    pub fn get<Q: EncodeLike<K>>(&self, key: Q) -> Option<V> {
        let length = read(&(PREFIX, &key, 0u32))?;
        Some(decode(length, |index| read(&(PREFIX, &key, index + 1))))
    }
    pub fn contains<Q: EncodeLike<K>>(&self, key: Q) -> bool {
        ink::env::contains_contract_storage(&(PREFIX, key, 0u32)).is_some()
    }
    pub fn insert<Q: EncodeLike<K>, R: EncodeLike<V> + Encode>(&mut self, key: Q, value: &R) {
        let old_length: u32 = read(&(PREFIX, &key, 0u32)).unwrap_or(0);
        let length = encode(value, |index, bytes| {
            write(&(PREFIX, &key, index + 1), &Bytes(bytes))
        });
        for index in count(length as usize)..count(old_length as usize) {
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
        let large = ink::prelude::vec![7u8;40_000];
        let old = save(0, &large);
        assert_eq!(load::<Vec<u8>>(old), large);
        for index in 0..count(old as usize) {
            assert!(page(old, index as u32).unwrap().len() <= CHUNK_BYTES);
        }
        let short = ink::prelude::vec![8u8;9];
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
        assert!(ink::env::contains_contract_storage(&(1u32, key, 1u32)).unwrap() < 16 * 1024);
        left.insert(key, &ink::prelude::vec![2u8]);
        assert!(ink::env::contains_contract_storage(&(1u32, key, 2u32)).is_none());
        left.remove(key);
        assert!(!left.contains(key));
        assert_eq!(left.get(key), None);
        assert!(ink::env::contains_contract_storage(&(1u32, key, 1u32)).is_none());
        assert!(right.contains(key));
    }
}
