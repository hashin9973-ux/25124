use bevy::prelude::*;

use crate::config::RuntimeConfig;

pub struct RenderingPlugin;

impl Plugin for RenderingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_key_light);
    }
}

fn setup_key_light(mut commands: Commands, config: Res<RuntimeConfig>) {
    let rotation = config.engine.rendering.key_light_rotation_degrees_xyz;
    commands.spawn((
        DirectionalLight {
            illuminance: config.engine.rendering.key_light_illuminance_lux,
            shadow_maps_enabled: config.engine.rendering.shadows_enabled,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(
            EulerRot::XYZ,
            rotation[0].to_radians(),
            rotation[1].to_radians(),
            rotation[2].to_radians(),
        )),
    ));
}
