use avian3d::{
    dynamics::solver::SolverConfig,
    prelude::{Gravity, PhysicsPlugins, SubstepCount, TimeToSleep},
};
use bevy::prelude::*;

#[derive(States, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum AppMode {
    #[default]
    WorldLibrary,
    Simulation,
}

use crate::{
    camera::CameraPlugin,
    config::RuntimeConfig,
    persistence::{PersistencePlugin, WorldSaveTarget},
    physics::ContactPlugin,
    rendering::RenderingPlugin,
    ui::MenuPlugin,
    world::WorldPlugin,
};

pub struct EnginePlugin;

impl Plugin for EnginePlugin {
    fn build(&self, app: &mut App) {
        let config = app.world().resource::<RuntimeConfig>().clone();
        let physics = &config.engine.physics;
        let gravity = config.world.gravity_m_s2.unwrap_or(physics.gravity_m_s2);

        app.init_state::<AppMode>()
            .add_plugins(PhysicsPlugins::default())
            .insert_resource(Time::<Fixed>::from_hz(physics.fixed_hz))
            .insert_resource(SubstepCount(physics.substeps))
            .insert_resource(TimeToSleep(physics.sleep_after_seconds))
            .insert_resource(Gravity(Vec3::from_array(gravity)))
            .insert_resource(SolverConfig {
                restitution_threshold: physics.restitution_threshold_m_s,
                restitution_iterations: physics.restitution_iterations as usize,
                contact_damping_ratio: physics.contact_damping_ratio,
                contact_frequency_factor: physics.contact_frequency_factor,
                max_overlap_solve_speed: physics.max_overlap_solve_speed_m_s,
                warm_start_coefficient: physics.warm_start_coefficient,
            })
            .insert_resource(WorldSaveTarget {
                world_id: config.world.id.clone(),
                state_path: config.paths.world_state.clone(),
            })
            .add_systems(Update, sync_physics_settings)
            .add_plugins((
                WorldPlugin,
                RenderingPlugin,
                CameraPlugin,
                ContactPlugin,
                PersistencePlugin,
                MenuPlugin,
            ));
    }
}

/// Re-apply per-world engine settings after the world library replaces RuntimeConfig.
fn sync_physics_settings(
    config: Res<RuntimeConfig>,
    mut fixed_time: ResMut<Time<Fixed>>,
    mut substeps: ResMut<SubstepCount>,
    mut time_to_sleep: ResMut<TimeToSleep>,
    mut gravity: ResMut<Gravity>,
    mut solver: ResMut<SolverConfig>,
) {
    if !config.is_changed() {
        return;
    }

    let physics = &config.engine.physics;
    *fixed_time = Time::<Fixed>::from_hz(physics.fixed_hz);
    substeps.0 = physics.substeps;
    time_to_sleep.0 = physics.sleep_after_seconds;
    gravity.0 = Vec3::from_array(
        config
            .world
            .gravity_m_s2
            .unwrap_or(physics.gravity_m_s2),
    );
    *solver = SolverConfig {
        restitution_threshold: physics.restitution_threshold_m_s,
        restitution_iterations: physics.restitution_iterations as usize,
        contact_damping_ratio: physics.contact_damping_ratio,
        contact_frequency_factor: physics.contact_frequency_factor,
        max_overlap_solve_speed: physics.max_overlap_solve_speed_m_s,
        warm_start_coefficient: physics.warm_start_coefficient,
    };
}
