//! A cache of storage reads for the life of one message.
use core::convert::Infallible;
use ink::prelude::vec::Vec;

/// The values a message has read, by key.
///
/// Start from `default()` and read each cell through [`Self::get_or`] or
/// [`Self::get_or_try`]: the closure runs only the first time a key is asked
/// for. Entries are never dropped or read again, so a value that changes in
/// storage must be changed here too, through the reference those return.
///
/// It is a sorted vector, not a `BTreeMap`:
///
/// - **For:** smaller contract code. A `BTreeMap` is compiled again for each
///   key and value type.
/// - **Against:** an insert shifts the entries after it. It suits the tens of
///   entries a message reads, not thousands.
pub(crate) struct Seen<K, V>(Vec<(K, V)>);

impl<K, V> Default for Seen<K, V> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<K: Ord, V> Seen<K, V> {
    /// The value seen for `key`, if any. Reads nothing.
    pub(crate) fn get(&self, key: &K) -> Option<&V> {
        let index = self.0.binary_search_by(|(seen, _)| seen.cmp(key)).ok()?;
        Some(&self.0[index].1)
    }

    /// The value seen for `key`, reading it with `read` the first time.
    pub(crate) fn get_or_try<E>(
        &mut self,
        key: K,
        read: impl FnOnce() -> Result<V, E>,
    ) -> Result<&mut V, E> {
        let index = match self.0.binary_search_by(|(seen, _)| seen.cmp(&key)) {
            Ok(index) => index,
            Err(index) => {
                self.0.insert(index, (key, read()?));
                index
            }
        };
        Ok(&mut self.0[index].1)
    }

    /// [`Self::get_or_try`] for a read that cannot fail.
    pub(crate) fn get_or(&mut self, key: K, read: impl FnOnce() -> V) -> &mut V {
        match self.get_or_try(key, || Ok::<_, Infallible>(read())) {
            Ok(value) => value,
            Err(never) => match never {},
        }
    }
}

impl<K, V> IntoIterator for Seen<K, V> {
    type Item = (K, V);
    type IntoIter = ink::prelude::vec::IntoIter<(K, V)>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
