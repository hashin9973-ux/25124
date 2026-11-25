use std::collections::HashSet;

use avian3d::prelude::{CollisionEnd, CollisionStart};
use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ContactPair {
    pub collider_a: Entity,
    pub collider_b: Entity,
}

impl ContactPair {
    fn new(first: Entity, second: Entity) -> Self {
        if first.to_bits() <= second.to_bits() {
            Self {
                collider_a: first,
                collider_b: second,
            }
        } else {
            Self {
                collider_a: second,
                collider_b: first,
            }
        }
    }
}

#[derive(Resource, Debug, Default)]
pub struct ContactLedger {
    pub active_pairs: HashSet<ContactPair>,
    pub total_started: u64,
    pub total_ended: u64,
}

impl ContactLedger {
    pub fn active_count(&self) -> usize {
        self.active_pairs.len()
    }
}

pub(super) fn collect_contact_transitions(
    mut starts: MessageReader<CollisionStart>,
    mut ends: MessageReader<CollisionEnd>,
    mut ledger: ResMut<ContactLedger>,
) {
    for event in starts.read() {
        if ledger
            .active_pairs
            .insert(ContactPair::new(event.collider1, event.collider2))
        {
            ledger.total_started += 1;
        }
    }

    for event in ends.read() {
        if ledger
            .active_pairs
            .remove(&ContactPair::new(event.collider1, event.collider2))
        {
            ledger.total_ended += 1;
        }
    }
}
