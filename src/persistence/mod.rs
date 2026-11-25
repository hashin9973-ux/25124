mod state;

pub use state::WorldStateFile;

use std::path::PathBuf;

use avian3d::prelude::{AngularVelocity, LinearVelocity};
use bevy::prelude::*;

use crate::persistence::state::PersistedBodyState;

#[derive(Component, Clone, Debug)]
pub struct PersistentObjectId(pub String);

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

fn save_world_on_f5(
    keys: Res<ButtonInput<KeyCode>>,
    target: Res<WorldSaveTarget>,
    bodies: Query<(
        &PersistentObjectId,
        &Transform,
        &LinearVelocity,
        &AngularVelocity,
    )>,
) {
    if !keys.just_pressed(KeyCode::F5) {
        return;
    }

    let mut snapshot = WorldStateFile::new(target.world_id.clone());
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
            "Saved {} rigid bodies to '{}'",
            snapshot.bodies.len(),
            target.state_path.display()
        ),
        Err(error) => error!(
            "Could not save world state to '{}': {error:#}",
            target.state_path.display()
        ),
    }
}
