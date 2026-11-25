use bevy::prelude::*;

use crate::config::RuntimeConfig;

pub struct RenderingPlugin;

impl Plugin for RenderingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_key_light);
    }
}

fn setup_key_light(mut commands: Commands, config: Res<RuntimeConfig>) {
    commands.spawn((
        DirectionalLight {
            illuminance: config.engine.rendering.key_light_illuminance_lux,
            shadows_enabled: config.engine.rendering.shadows_enabled,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.85, -0.55, 0.0)),
    ));
}
