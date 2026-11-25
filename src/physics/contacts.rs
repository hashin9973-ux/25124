use std::collections::HashSet;

use avian3d::prelude::{CollisionEnd, CollisionStart, Collisions};
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
    collisions: Collisions,
    mut ledger: ResMut<ContactLedger>,
) {
    // The two transition message streams do not share one chronological queue. Count every
    // transition, but derive the active set from Avian's final contact graph for this frame.
    ledger.total_started += starts.read().count() as u64;
    ledger.total_ended += ends.read().count() as u64;
    ledger.active_pairs = collisions
        .iter()
        .map(|pair| ContactPair::new(pair.collider1, pair.collider2))
        .collect();
}

#[cfg(test)]
mod tests {
    use bevy::prelude::Entity;

    use super::ContactPair;

    #[test]
    fn contact_pair_order_is_canonical() {
        let first = Entity::from_bits(1);
        let second = Entity::from_bits(2);
        assert_eq!(
            ContactPair::new(first, second),
            ContactPair::new(second, first)
        );
    }
}
