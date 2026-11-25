//! The public IDs in use, across every kind of entity.
use crate::dao::{require, Id, Result};
use ink::prelude::collections::BTreeSet;
use ink::storage::{traits::ManualKey, Mapping};

/// An ID an entity wants, with the error to fail with if it is taken.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Claim {
    pub(crate) id: Id,
    pub(crate) error_if_taken: &'static str,
}

/// One entry per public ID in use. IDs are unique across every kind of
/// entity: [`Self::claim`] one when creating what it names, and
/// [`Self::replace`] the set an owner holds when that set changes.
#[ink::storage_item]
#[derive(Default, Debug)]
pub(crate) struct PublicIds {
    taken: Mapping<Id, bool, ManualKey<29>>,
}

impl PublicIds {
    /// Take `id`, or fail with `error_if_taken` if an entity already has it.
    pub(crate) fn claim(&mut self, id: Id, error_if_taken: &str) -> Result<()> {
        // Writing an entry answers whether there was one. On a collision the
        // message fails and the write is reverted with the rest of it.
        require(self.taken.insert(id, &true).is_none(), error_if_taken)
    }

    /// Replace the IDs an owner held, `old`, with `new`: free the ones it no
    /// longer uses and take the ones it adds.
    pub(crate) fn replace(&mut self, old: &[Claim], new: &[Claim]) -> Result<()> {
        let mut kept = BTreeSet::new();
        for claim in new {
            require(kept.insert(claim.id), claim.error_if_taken)?;
        }
        for held in old {
            if !kept.contains(&held.id) {
                self.taken.remove(held.id);
            }
        }
        for claim in new {
            if !old.iter().any(|held| held.id == claim.id) {
                self.claim(claim.id, claim.error_if_taken)?;
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, id: Id) -> bool {
        self.taken.contains(id)
    }
}
