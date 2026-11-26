use avian3d::{
    dynamics::solver::SolverConfig,
    prelude::{Gravity, PhysicsPlugins, SubstepCount},
};
use bevy::prelude::*;

use crate::{
    camera::CameraPlugin,
    config::RuntimeConfig,
    persistence::{PersistencePlugin, WorldSaveTarget},
    physics::ContactPlugin,
    rendering::RenderingPlugin,
    world::WorldPlugin,
};

pub struct EnginePlugin;

impl Plugin for EnginePlugin {
    fn build(&self, app: &mut App) {
        let config = app.world().resource::<RuntimeConfig>().clone();
        let gravity = config
            .world
            .gravity_m_s2
            .unwrap_or(config.engine.physics.gravity_m_s2);

        app.add_plugins(PhysicsPlugins::default())
            .insert_resource(Time::<Fixed>::from_hz(config.engine.physics.fixed_hz))
            .insert_resource(SubstepCount(config.engine.physics.substeps))
            .insert_resource(SolverConfig {
                restitution_threshold: config.engine.physics.restitution_threshold_m_s,
                ..default()
            })
            .insert_resource(Gravity(Vec3::from_array(gravity)))
            .insert_resource(WorldSaveTarget {
                world_id: config.world.id.clone(),
                state_path: config.paths.world_state.clone(),
            })
            .add_plugins((
                WorldPlugin,
                RenderingPlugin,
                CameraPlugin,
                ContactPlugin,
                PersistencePlugin,
            ))
            .add_systems(Update, sync_world_physics_settings);
    }
}

fn sync_world_physics_settings(
    config: Res<RuntimeConfig>,
    mut fixed_time: ResMut<Time<Fixed>>,
    mut substeps: ResMut<SubstepCount>,
    mut solver: ResMut<SolverConfig>,
) {
    if !config.is_changed() {
        return;
    }

    *fixed_time = Time::<Fixed>::from_hz(config.engine.physics.fixed_hz);
    *substeps = SubstepCount(config.engine.physics.substeps);
    solver.restitution_threshold = config.engine.physics.restitution_threshold_m_s;
}
