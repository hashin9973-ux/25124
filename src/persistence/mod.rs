mod state;

pub use state::WorldStateFile;

use std::{
    collections::{BTreeMap, HashSet},
    path::PathBuf,
};

use avian3d::prelude::{AngularVelocity, LinearVelocity};
use bevy::prelude::*;

use crate::persistence::state::PersistedBodyState;

#[derive(Component, Clone, Debug)]
pub struct PersistentObjectId(pub String);

/// Marks bodies whose runtime transform and velocities override authored world placement.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct PersistRuntimeState;

#[derive(Resource, Clone, Debug)]
pub struct WorldSaveTarget {
    pub world_id: String,
    pub state_path: PathBuf,
}

pub struct PersistencePlugin;

impl Plugin for PersistencePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, save_world_on_f5);
    }
}

fn unresolved_body_states(
    previous_state: &WorldStateFile,
    known_instance_ids: &HashSet<&str>,
) -> BTreeMap<String, PersistedBodyState> {
    previous_state
        .bodies
        .iter()
        .filter(|(id, _)| !known_instance_ids.contains(id.as_str()))
        .map(|(id, state)| (id.clone(), state.clone()))
        .collect()
}

fn save_world_on_f5(
    keys: Res<ButtonInput<KeyCode>>,
    target: Res<WorldSaveTarget>,
    config: Res<crate::config::RuntimeConfig>,
    previous_state: Res<WorldStateFile>,
    bodies: Query<
        (
            &PersistentObjectId,
            &Transform,
            &LinearVelocity,
            &AngularVelocity,
        ),
        With<PersistRuntimeState>,
    >,
) {
    if !keys.just_pressed(KeyCode::F5) {
        return;
    }

    let known_instance_ids = config
        .world
        .objects
        .iter()
        .map(|instance| instance.id.as_str())
        .collect::<HashSet<_>>();

    // Preserve unresolved records so temporarily unavailable content can be restored later.
    // Known static-body records are deliberately omitted: static placement belongs to the manifest.
    let mut snapshot = WorldStateFile::new(target.world_id.clone());
    snapshot.bodies = unresolved_body_states(&previous_state, &known_instance_ids);

    for (id, transform, linear_velocity, angular_velocity) in &bodies {
        snapshot.bodies.insert(
            id.0.clone(),
            PersistedBodyState {
                translation_m: transform.translation.to_array(),
                rotation_xyzw: transform.rotation.to_array(),
                linear_velocity_m_s: linear_velocity.0.to_array(),
                angular_velocity_rad_s: angular_velocity.0.to_array(),
            },
        );
    }

    match snapshot.write_atomic(&target.state_path) {
        Ok(()) => info!(
            "Saved {} runtime body states (plus unresolved records) to '{}'",
            snapshot.bodies.len(),
            target.state_path.display()
        ),
        Err(error) => error!(
            "Could not save world state to '{}': {error:#}",
            target.state_path.display()
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{PersistedBodyState, WorldStateFile, unresolved_body_states};

    #[test]
    fn save_snapshot_preserves_only_records_not_owned_by_current_world_instances() {
        let mut previous = WorldStateFile::new("test-world".to_owned());
        for id in ["dynamic", "static", "unresolved-mod-object"] {
            previous.bodies.insert(
                id.to_owned(),
                PersistedBodyState {
                    translation_m: [1.0, 2.0, 3.0],
                    rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
                    linear_velocity_m_s: [0.0; 3],
                    angular_velocity_rad_s: [0.0; 3],
                },
            );
        }

        let known_instance_ids = HashSet::from(["dynamic", "static"]);
        let unresolved = unresolved_body_states(&previous, &known_instance_ids);

        assert_eq!(unresolved.len(), 1);
        assert!(unresolved.contains_key("unresolved-mod-object"));
        assert!(!unresolved.contains_key("static"));
    }
}
