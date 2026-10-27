//! Catalog entries, role associations and skill requests.
use crate::dao::*;
use crate::indices;
use ink::codegen::Env as _;
use ink::prelude::{string::String, vec::Vec};
use ink::primitives::AccountId;
use ink::storage::{traits::ManualKey, Mapping, StorageVec};

#[derive(Clone, Debug, scale::Encode, scale::Decode)]
#[cfg_attr(
    feature = "std",
    derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
)]
pub struct RequestRecord {
    pub request_id: Id,
    pub requester: AccountId,
    pub content: CatalogReferenceV1,
    pub roles: u32,
    pub status: SkillRequestStatus,
    pub skill_id: Option<u32>,
}

#[ink::storage_item]
#[derive(Default, Debug)]
pub(crate) struct Catalog {
    pub(crate) roles: Mapping<u32, CatalogEntry, ManualKey<1>>,
    pub(crate) role_ids: StorageVec<u32, ManualKey<50>>,
    pub(crate) skills: Mapping<u32, CatalogEntry, ManualKey<2>>,
    pub(crate) skill_ids: StorageVec<u32, ManualKey<51>>,
    pub(crate) skill_roles: Mapping<u32, u32, ManualKey<3>>,
    pub(crate) requests: Mapping<Id, RequestRecord, ManualKey<6>>,
    pub(crate) request_ids: StorageVec<Id, ManualKey<53>>,
    pub(crate) origins: Mapping<u32, CatalogReferenceV1, ManualKey<18>>,
    pub(crate) skill_role_values: Mapping<(u32, u32), u32, ManualKey<20>>,
    pub(crate) request_roles: Mapping<(Id, u32), u32, ManualKey<23>>,
}

impl DaoData {
    pub(crate) fn qualifications(&self, qualifications: &Qualifications) -> Result<()> {
        qualifications.validate()?;
        require(
            qualifications
                .role_ids
                .iter()
                .all(|id| self.catalog.roles.contains(id))
                && qualifications
                    .skill_ids
                    .iter()
                    .all(|id| self.catalog.skills.contains(id)),
            "unknown_qualification",
        )
    }

    pub fn validate_qualifications(&self, qualifications: Qualifications) -> Result<()> {
        self.qualifications(&qualifications)
    }

    pub fn catalog(&self) -> CatalogSeed {
        CatalogSeed {
            roles: indices::values(&self.catalog.role_ids)
                .iter()
                .filter_map(|id| self.catalog.roles.get(id))
                .collect(),
            skills: indices::values(&self.catalog.skill_ids)
                .iter()
                .filter_map(|id| self.catalog.skills.get(id))
                .collect(),
            skill_roles: indices::values(&self.catalog.skill_ids)
                .iter()
                .map(|id| SkillRoleAssociation {
                    skill_id: *id,
                    role_ids: self.skill_role_ids(*id),
                    origin: self.catalog.origins.get(id).expect("skill origin missing"),
                })
                .collect(),
        }
    }

    pub fn skill_requests(&self) -> Vec<SkillRequestView> {
        let caller = self.env().caller();
        indices::values(&self.catalog.request_ids)
            .iter()
            .filter_map(|id| self.request_record(*id))
            .filter(|request| caller == self.config.root || caller == request.requester)
            .collect()
    }

    pub fn upsert_catalog_entry(
        &mut self,
        kind: CatalogKind,
        id: u32,
        content: MetadataCommitmentV1,
    ) -> Result<Effect> {
        require(id != 0, "invalid_contract")?;
        self.root()?;
        require(
            !(kind == CatalogKind::Role && id == 1),
            "fixed_coordinator_role",
        )?;
        let entry = CatalogEntry {
            id,
            fixed: false,
            content: CatalogReferenceV1::Write(content),
        };
        match kind {
            CatalogKind::Role => {
                self.catalog.roles.insert(id, &entry);
                insert_sorted(&mut self.catalog.role_ids, id);
            }
            CatalogKind::Skill => {
                self.catalog.skills.insert(id, &entry);
                insert_sorted(&mut self.catalog.skill_ids, id);
                if !self.catalog.skill_roles.contains(id) {
                    self.write_skill_roles(id, &[]);
                    self.catalog.origins.insert(id, &entry.content);
                }
            }
        }
        Ok(Effect::new(3, None, self.env().caller()))
    }

    pub fn delete_catalog_entry(&mut self, kind: CatalogKind, id: u32) -> Result<Effect> {
        require(id != 0, "invalid_contract")?;
        self.root()?;
        require(
            !(kind == CatalogKind::Role && id == 1),
            "fixed_coordinator_role",
        )?;
        let references = |qualifications: &Qualifications| match kind {
            CatalogKind::Role => qualifications.role_ids.contains(&id),
            CatalogKind::Skill => qualifications.skill_ids.contains(&id),
        };
        require(
            !indices::values(&self.members.accounts)
                .iter()
                .any(|account| {
                    self.members
                        .workers
                        .get(account)
                        .map(|_| {
                            references(
                                &self
                                    .worker_with_qualifications(*account)
                                    .expect("invalid worker record")
                                    .qualifications,
                            )
                        })
                        .unwrap_or(false)
                })
                && !indices::values(&self.projects.proposal_ids)
                    .iter()
                    .any(|proposal| {
                        self.proposal_qualifications(*proposal)
                            .map(|q| references(&q))
                            .unwrap_or(false)
                    }),
            "catalog_entry_in_use",
        )?;
        match kind {
            CatalogKind::Role => {
                require(
                    !indices::values(&self.catalog.skill_ids)
                        .iter()
                        .any(|skill| self.skill_role_ids(*skill).contains(&id)),
                    "catalog_entry_in_use",
                )?;
                require(self.catalog.roles.contains(id), "catalog_entry_not_found")?;
                self.catalog.roles.remove(id);
                indices::remove(&mut self.catalog.role_ids, &id);
            }
            CatalogKind::Skill => {
                require(self.catalog.skills.contains(id), "catalog_entry_not_found")?;
                self.catalog.skills.remove(id);
                self.remove_skill_roles(id);
                self.catalog.origins.remove(id);
                indices::remove(&mut self.catalog.skill_ids, &id);
            }
        }
        Ok(Effect::new(3, None, self.env().caller()))
    }

    pub fn create_skill_request(
        &mut self,
        role_ids: Vec<u32>,
        content: MetadataCommitmentV1,
        request_id: Id,
    ) -> Result<Effect> {
        require(!role_ids.is_empty(), "invalid_contract")?;
        valid_ids(&role_ids)?;
        let origin = self.env().caller();
        self.worker_record(origin)?;
        require(
            role_ids.iter().all(|id| self.catalog.roles.contains(id)),
            "unknown_role",
        )?;
        require(
            !self.catalog.requests.contains(request_id),
            "request_id_collision",
        )?;
        self.write_request(
            request_id,
            &SkillRequestView {
                request_id,
                requester: origin,
                content: CatalogReferenceV1::Write(content),
                role_ids,
                status: SkillRequestStatus::Pending,
                skill_id: None,
            },
        );
        insert_sorted(&mut self.catalog.request_ids, request_id);
        Ok(Effect::moderated(31, request_id, origin, self.config.root))
    }

    pub fn decide_skill_request(
        &mut self,
        request_id: Id,
        decision: SkillRequestDecision,
    ) -> Result<Effect> {
        self.root()?;
        let mut request = self
            .request_record(request_id)
            .ok_or_else(|| String::from("skill_request_not_found"))?;
        require(
            request.status == SkillRequestStatus::Pending,
            "skill_request_decided",
        )?;
        let kind = match decision {
            SkillRequestDecision::Approve => {
                require(
                    request
                        .role_ids
                        .iter()
                        .all(|id| self.catalog.roles.contains(id)),
                    "unknown_role",
                )?;
                let id = self
                    .catalog
                    .skill_ids
                    .peek()
                    .map_or(Some(1), |id| id.checked_add(1))
                    .ok_or_else(|| String::from("catalog_id_exhausted"))?;
                self.catalog.skills.insert(
                    id,
                    &CatalogEntry {
                        id,
                        fixed: false,
                        content: request.content.clone(),
                    },
                );
                insert_sorted(&mut self.catalog.skill_ids, id);
                self.write_skill_roles(id, &request.role_ids);
                self.catalog.origins.insert(id, &request.content);
                request.status = SkillRequestStatus::Approved;
                request.skill_id = Some(id);
                32
            }
            SkillRequestDecision::Reject => {
                request.status = SkillRequestStatus::Rejected;
                33
            }
        };
        self.write_request_header(request_id, &request);
        Ok(Effect::moderated(
            kind,
            request_id,
            request.requester,
            self.config.root,
        ))
    }

    pub(crate) fn skill_role_ids(&self, skill: u32) -> Vec<u32> {
        (0..self.catalog.skill_roles.get(skill).unwrap_or(0))
            .map(|i| {
                self.catalog
                    .skill_role_values
                    .get((skill, i))
                    .expect("missing skill role")
            })
            .collect()
    }

    pub(crate) fn remove_skill_roles(&mut self, skill: u32) {
        for i in 0..self.catalog.skill_roles.get(skill).unwrap_or(0) {
            self.catalog.skill_role_values.remove((skill, i));
        }
        self.catalog.skill_roles.remove(skill);
    }

    pub(crate) fn write_skill_roles(&mut self, skill: u32, roles: &[u32]) {
        self.remove_skill_roles(skill);
        for (i, role) in roles.iter().enumerate() {
            self.catalog
                .skill_role_values
                .insert((skill, count(i)), role);
        }
        self.catalog.skill_roles.insert(skill, &count(roles.len()));
    }

    pub(crate) fn request_record(&self, id: Id) -> Option<SkillRequestView> {
        self.catalog
            .requests
            .get(id)
            .map(|record| SkillRequestView {
                request_id: record.request_id,
                requester: record.requester,
                content: record.content,
                role_ids: (0..record.roles)
                    .map(|i| {
                        self.catalog
                            .request_roles
                            .get((id, i))
                            .expect("missing request role")
                    })
                    .collect(),
                status: record.status,
                skill_id: record.skill_id,
            })
    }

    pub(crate) fn write_request_header(&mut self, id: Id, record: &SkillRequestView) {
        self.catalog.requests.insert(
            id,
            &RequestRecord {
                request_id: record.request_id,
                requester: record.requester,
                content: record.content.clone(),
                roles: count(record.role_ids.len()),
                status: record.status,
                skill_id: record.skill_id,
            },
        );
    }

    pub(crate) fn write_request(&mut self, id: Id, record: &SkillRequestView) {
        if let Some(old) = self.catalog.requests.get(id) {
            for i in 0..old.roles {
                self.catalog.request_roles.remove((id, i));
            }
        }
        for (i, role) in record.role_ids.iter().enumerate() {
            self.catalog.request_roles.insert((id, count(i)), role);
        }
        self.write_request_header(id, record);
    }
}

impl DaoData {
    pub(crate) fn seed_catalog(&mut self, seed: CatalogSeed) -> Result<()> {
        for role in seed.roles {
            require(
                role.id != 0 && !self.catalog.roles.contains(role.id),
                "invalid_catalog_state",
            )?;
            require(role.fixed == (role.id == 1), "invalid_catalog_state")?;
            insert_sorted(&mut self.catalog.role_ids, role.id);
            self.catalog.roles.insert(role.id, &role);
        }
        require(self.catalog.roles.contains(1), "invalid_catalog_state")?;
        for skill in seed.skills {
            require(
                skill.id != 0 && !skill.fixed && !self.catalog.skills.contains(skill.id),
                "invalid_catalog_state",
            )?;
            insert_sorted(&mut self.catalog.skill_ids, skill.id);
            self.catalog.skills.insert(skill.id, &skill);
        }
        for association in seed.skill_roles {
            require(
                self.catalog.skills.contains(association.skill_id)
                    && !self.catalog.skill_roles.contains(association.skill_id)
                    && association
                        .role_ids
                        .iter()
                        .all(|id| self.catalog.roles.contains(id)),
                "invalid_catalog_state",
            )?;
            self.write_skill_roles(association.skill_id, &association.role_ids);
            self.catalog
                .origins
                .insert(association.skill_id, &association.origin);
        }
        require(
            indices::values(&self.catalog.skill_ids)
                .iter()
                .all(|id| self.catalog.skill_roles.contains(id)),
            "invalid_catalog_state",
        )?;
        Ok(())
    }
}
