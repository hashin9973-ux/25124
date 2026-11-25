use bevy::prelude::*;

use crate::camera::FreeFlyCamera;

use super::{ActiveWorld, MenuAction, PendingMenuCommand, WorldKind};

#[derive(Component)]
struct StudioCallout {
    action: MenuAction,
    hit_extent: Vec2,
}

pub(super) fn ensure_studio_callouts(
    mut commands: Commands,
    active_world: Res<ActiveWorld>,
    cameras: Query<Entity, With<FreeFlyCamera>>,
    callouts: Query<Entity, With<StudioCallout>>,
) {
    if active_world.kind != Some(WorldKind::CharacterStudio) || !callouts.is_empty() {
        return;
    }

    let Some(camera) = cameras.iter().next() else {
        return;
    };

    let mut callout = commands.spawn((
        Name::new("Create species annotation"),
        StudioCallout {
            action: MenuAction::CreateSpecies,
            hit_extent: Vec2::new(230.0, 58.0),
        },
        Transform::from_xyz(0.0, -0.65, -3.0),
    ));
    callout.with_children(|parent| {
        for offset in [
            Vec3::new(-0.04, 0.0, -0.03),
            Vec3::new(0.04, 0.0, -0.03),
            Vec3::new(0.0, -0.04, -0.03),
            Vec3::new(0.0, 0.04, -0.03),
            Vec3::new(-0.03, -0.03, -0.03),
            Vec3::new(0.03, 0.03, -0.03),
        ] {
            parent.spawn((
                Text2d::new("CREATE SPECIES"),
                TextFont::from_font_size(48.0),
                TextColor(Color::BLACK),
                Anchor::Center,
                Transform::from_translation(offset),
            ));
        }
        parent.spawn((
            Text2d::new("CREATE SPECIES"),
            TextFont::from_font_size(48.0),
            TextColor(Color::WHITE),
            Anchor::Center,
            Transform::from_xyz(0.0, 0.0, 0.025),
        ));
    });

    commands.entity(camera).add_child(callout.id());
}

pub(super) fn interact_callouts(
    window: Single<&Window, With<bevy::window::PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<FreeFlyCamera>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut callouts: Query<(&StudioCallout, &GlobalTransform, &mut Transform)>,
    mut pending: ResMut<PendingMenuCommand>,
) {
    let cursor = window.cursor_position();
    let (camera, camera_transform) = *camera;

    for (callout, global_transform, mut transform) in &mut callouts {
        let hovered = cursor.is_some_and(|cursor| {
            camera
                .world_to_viewport(camera_transform, global_transform.translation())
                .is_ok_and(|screen_position| {
                    let offset = (cursor - screen_position).abs();
                    offset.x <= callout.hit_extent.x && offset.y <= callout.hit_extent.y
                })
        });

        let target_scale = if hovered { 1.16 } else { 1.0 };
        transform.scale = transform.scale.lerp(Vec3::splat(target_scale), 0.22);

        if hovered && mouse.just_pressed(MouseButton::Left) {
            pending.0 = Some(callout.action.clone());
        }
    }
}

