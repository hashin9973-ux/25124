use bevy::prelude::*;

use crate::config::RuntimeConfig;

#[derive(Component)]
struct ProjectKeyLight;

pub struct RenderingPlugin;

impl Plugin for RenderingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_key_light)
            .add_systems(Update, sync_key_light);
    }
}

fn setup_key_light(mut commands: Commands, config: Res<RuntimeConfig>) {
    let rotation = config.engine.rendering.key_light_rotation_degrees_xyz;
    commands.spawn((
        ProjectKeyLight,
        DirectionalLight {
            color: Color::srgba(
                config.engine.rendering.key_light_color_srgba[0],
                config.engine.rendering.key_light_color_srgba[1],
                config.engine.rendering.key_light_color_srgba[2],
                config.engine.rendering.key_light_color_srgba[3],
            ),
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

fn sync_key_light(
    config: Res<RuntimeConfig>,
    mut lights: Query<(&mut DirectionalLight, &mut Transform), With<ProjectKeyLight>>,
) {
    if !config.is_changed() {
        return;
    }

    let rotation = config.engine.rendering.key_light_rotation_degrees_xyz;
    let transform_rotation = Quat::from_euler(
        EulerRot::XYZ,
        rotation[0].to_radians(),
        rotation[1].to_radians(),
        rotation[2].to_radians(),
    );

    for (mut light, mut transform) in &mut lights {
        light.color = Color::srgba(
            config.engine.rendering.key_light_color_srgba[0],
            config.engine.rendering.key_light_color_srgba[1],
            config.engine.rendering.key_light_color_srgba[2],
            config.engine.rendering.key_light_color_srgba[3],
        );
        light.illuminance = config.engine.rendering.key_light_illuminance_lux;
        light.shadow_maps_enabled = config.engine.rendering.shadows_enabled;
        transform.rotation = transform_rotation;
    }
}
