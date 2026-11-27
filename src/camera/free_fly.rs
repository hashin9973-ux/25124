use bevy::{
    input::mouse::AccumulatedMouseMotion,
    prelude::*,
    window::{CursorGrabMode, CursorOptions},
};

use crate::config::RuntimeConfig;

#[derive(Component, Debug)]
pub struct FreeFlyCamera {
    yaw_radians: f32,
    pitch_radians: f32,
}

pub(super) fn spawn_camera(mut commands: Commands, config: Res<RuntimeConfig>) {
    let camera = &config.engine.camera;
    let yaw = camera.yaw_degrees.to_radians();
    let pitch = camera.pitch_degrees.to_radians();

    commands.spawn((
        Camera3d::default(),
        FreeFlyCamera {
            yaw_radians: yaw,
            pitch_radians: pitch,
        },
        Transform {
            translation: Vec3::from_array(camera.spawn_position_m),
            rotation: Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0),
            ..default()
        },
    ));
}

pub(super) fn move_camera(
    keys: Res<ButtonInput<KeyCode>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    time: Res<Time>,
    config: Res<RuntimeConfig>,
    menu: Res<crate::ui::MenuState>,
    mut cameras: Query<(&mut Transform, &mut FreeFlyCamera)>,
) {
    if menu.visible {
        return;
    }

    let settings = &config.engine.camera;
    let delta = if mouse_buttons.pressed(MouseButton::Right) {
        mouse_motion.delta * settings.mouse_radians_per_pixel
    } else {
        Vec2::ZERO
    };

    for (mut transform, mut controller) in &mut cameras {
        controller.yaw_radians -= delta.x;
        controller.pitch_radians =
            (controller.pitch_radians - delta.y).clamp(-1.553_343, 1.553_343);

        transform.rotation = Quat::from_euler(
            EulerRot::YXZ,
            controller.yaw_radians,
            controller.pitch_radians,
            0.0,
        );

        let forward = transform.forward().as_vec3();
        let right = transform.right().as_vec3();
        let mut movement = Vec3::ZERO;

        if keys.pressed(KeyCode::KeyW) {
            movement += forward;
        }
        if keys.pressed(KeyCode::KeyS) {
            movement -= forward;
        }
        if keys.pressed(KeyCode::KeyD) {
            movement += right;
        }
        if keys.pressed(KeyCode::KeyA) {
            movement -= right;
        }
        if keys.pressed(KeyCode::Space) {
            movement += Vec3::Y;
        }
        if keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight) {
            movement -= Vec3::Y;
        }

        let sprint = if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
            settings.sprint_multiplier
        } else {
            1.0
        };

        transform.translation +=
            movement.normalize_or_zero() * settings.move_speed_m_s * sprint * time.delta_secs();
    }
}

/// Reposition the already-created camera when the selected world's settings change.
pub(super) fn sync_camera_settings(
    config: Res<RuntimeConfig>,
    mut cameras: Query<(&mut Transform, &mut FreeFlyCamera)>,
) {
    if !config.is_changed() {
        return;
    }

    let camera = &config.engine.camera;
    let yaw = camera.yaw_degrees.to_radians();
    let pitch = camera.pitch_degrees.to_radians().clamp(-1.553_343, 1.553_343);
    for (mut transform, mut controller) in &mut cameras {
        controller.yaw_radians = yaw;
        controller.pitch_radians = pitch;
        transform.translation = Vec3::from_array(camera.spawn_position_m);
        transform.rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
    }
}

/// Right mouse button temporarily captures the cursor for predictable camera look.
pub(super) fn update_cursor_grab(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    menu: Res<crate::ui::MenuState>,
    mut cursors: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    let Ok(mut cursor) = cursors.single_mut() else {
        return;
    };

    let captured = !menu.visible && mouse_buttons.pressed(MouseButton::Right);
    let (visible, grab_mode) = if captured {
        (false, CursorGrabMode::Locked)
    } else {
        (true, CursorGrabMode::None)
    };

    if cursor.visible != visible {
        cursor.visible = visible;
    }
    if cursor.grab_mode != grab_mode {
        cursor.grab_mode = grab_mode;
    }
}
