//! The public IDs in use, across every kind of entity.
use crate::dao::{require, Id, Result};
use ink::prelude::collections::BTreeSet;
use ink::storage::{traits::ManualKey, Mapping};

/// An ID with the error that reports it as taken.
pub(crate) type Claim = (Id, &'static str);

/// One entry per public ID. Public IDs are unique across kinds, and an entry
/// answers that with one storage access, where walking every worker, project,
/// deposit and dispute cost more proof with every one of them.
#[ink::storage_item]
#[derive(Default, Debug)]
pub(crate) struct PublicIds {
    taken: Mapping<Id, bool, ManualKey<29>>,
}

impl PublicIds {
    /// Take `id`, or fail with `collision` if an entity already has it.
    pub(crate) fn claim(&mut self, id: Id, collision: &str) -> Result<()> {
        // Writing an entry answers whether there was one. On a collision the
        // message fails and the write is reverted with the rest of it.
        require(self.taken.insert(id, &true).is_none(), collision)
    }

    /// Replace the IDs an owner held, `old`, with `new`: free the ones it no
    /// longer uses and take the ones it adds.
    pub(crate) fn exchange(&mut self, old: &[Claim], new: &[Claim]) -> Result<()> {
        let mut kept = BTreeSet::new();
        for (id, collision) in new {
            require(kept.insert(*id), collision)?;
        }
        for (id, _) in old {
            if !kept.contains(id) {
                self.taken.remove(id);
            }
        }
        for (id, collision) in new {
            if !old.iter().any(|(held, _)| held == id) {
                self.claim(*id, collision)?;
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, id: Id) -> bool {
        self.taken.contains(id)
    }
}
