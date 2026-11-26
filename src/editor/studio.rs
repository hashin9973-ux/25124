use bevy::prelude::*;
use bevy::sprite::Anchor;

use crate::camera::FreeFlyCamera;

use super::{
    ActiveWorld, MenuAction, MeshElementMode, MeshViewportState, PendingMenuCommand, WorldKind,
};

#[derive(Clone, Debug)]
enum StudioCalloutAction {
    Command(MenuAction),
    SelectMode(MeshElementMode),
}

#[derive(Component)]
pub(super) struct StudioHint;

#[derive(Component)]
pub(super) struct StudioCallout {
    action: StudioCalloutAction,
    hit_extent: Vec2,
}

pub(super) fn ensure_studio_callouts(
    mut commands: Commands,
    active_world: Res<ActiveWorld>,
    cameras: Query<Entity, With<FreeFlyCamera>>,
    callouts: Query<&StudioCallout>,
    hints: Query<(), With<StudioHint>>,
) {
    if active_world.kind != Some(WorldKind::CharacterStudio) {
        return;
    }

    let Some(camera) = cameras.iter().next() else {
        return;
    };

    let definitions = [
        (
            StudioCalloutAction::SelectMode(MeshElementMode::Vertex),
            "VERTEX",
            Vec3::new(-1.2, -0.45, -3.0),
            Vec2::new(68.0, 36.0),
        ),
        (
            StudioCalloutAction::SelectMode(MeshElementMode::Edge),
            "EDGE",
            Vec3::new(0.0, -0.45, -3.0),
            Vec2::new(60.0, 36.0),
        ),
        (
            StudioCalloutAction::SelectMode(MeshElementMode::Face),
            "FACE",
            Vec3::new(1.2, -0.45, -3.0),
            Vec2::new(60.0, 36.0),
        ),
        (
            StudioCalloutAction::Command(MenuAction::CreateSpecies),
            "CREATE SPECIES",
            Vec3::new(-0.8, -1.05, -3.0),
            Vec2::new(190.0, 42.0),
        ),
        (
            StudioCalloutAction::Command(MenuAction::CreateMesh),
            "CREATE MESH",
            Vec3::new(0.8, -1.05, -3.0),
            Vec2::new(150.0, 42.0),
        ),
    ];

    if hints.is_empty() {
        spawn_studio_hint(&mut commands, camera);
    }

    for (action, label, position, hit_extent) in definitions {
        let exists = callouts
            .iter()
            .any(|callout| same_callout_action(&callout.action, &action));
        if !exists {
            spawn_studio_callout(&mut commands, camera, action, label, position, hit_extent);
        }
    }
}

fn same_callout_action(first: &StudioCalloutAction, second: &StudioCalloutAction) -> bool {
    match (first, second) {
        (StudioCalloutAction::SelectMode(first), StudioCalloutAction::SelectMode(second)) => {
            first == second
        }
        (
            StudioCalloutAction::Command(MenuAction::CreateSpecies),
            StudioCalloutAction::Command(MenuAction::CreateSpecies),
        ) => true,
        (
            StudioCalloutAction::Command(MenuAction::CreateMesh),
            StudioCalloutAction::Command(MenuAction::CreateMesh),
        ) => true,
        _ => false,
    }
}

fn spawn_studio_hint(commands: &mut Commands, camera: Entity) {
    let hint = commands
        .spawn((
            Name::new("Mesh modeling keyboard hints"),
            StudioHint,
            Transform::from_xyz(0.0, -1.58, -3.0),
        ))
        .with_children(|parent| {
        for offset in [
            Vec3::new(-0.025, 0.0, -0.025),
            Vec3::new(0.025, 0.0, -0.025),
            Vec3::new(0.0, -0.025, -0.025),
            Vec3::new(0.0, 0.025, -0.025),
        ] {
            parent.spawn((
                Text2d::new("CTRL+Z UNDO   CTRL+Y / CTRL+SHIFT+Z REDO\nSHIFT+CLICK MULTI   ARROWS GRID MOVE   PGUP/DN NORMAL   DEL DELETE   E EDGE   F FACE   TAB NEXT"),
                TextFont::from_font_size(17.0),
                TextColor(Color::BLACK),
                Anchor::CENTER,
                Transform::from_translation(offset),
            ));
        }
        parent.spawn((
            Text2d::new("CTRL+Z UNDO   CTRL+Y / CTRL+SHIFT+Z REDO\nSHIFT+CLICK MULTI   ARROWS GRID MOVE   PGUP/DN NORMAL   DEL DELETE   E EDGE   F FACE   TAB NEXT"),
            TextFont::from_font_size(17.0),
            TextColor(Color::WHITE),
            Anchor::CENTER,
            Transform::from_xyz(0.0, 0.0, 0.025),
        ));
        })
        .id();
    commands.entity(camera).add_child(hint);
}

fn spawn_studio_callout(
    commands: &mut Commands,
    camera: Entity,
    action: StudioCalloutAction,
    label: &str,
    position: Vec3,
    hit_extent: Vec2,
) {
    let font_size = if label.starts_with("CREATE") {
        36.0
    } else {
        32.0
    };
    let callout = commands
        .spawn((
            Name::new(format!("{label} annotation")),
            StudioCallout { action, hit_extent },
            Transform::from_translation(position),
        ))
        .with_children(|parent| {
            for offset in [
                Vec3::new(-0.04, 0.0, -0.03),
                Vec3::new(0.04, 0.0, -0.03),
                Vec3::new(0.0, -0.04, -0.03),
                Vec3::new(0.0, 0.04, -0.03),
                Vec3::new(-0.03, -0.03, -0.03),
                Vec3::new(0.03, 0.03, -0.03),
            ] {
                parent.spawn((
                    Text2d::new(label),
                    TextFont::from_font_size(font_size),
                    TextColor(Color::BLACK),
                    Anchor::CENTER,
                    Transform::from_translation(offset),
                ));
            }
            parent.spawn((
                Text2d::new(label),
                TextFont::from_font_size(font_size),
                TextColor(Color::WHITE),
                Anchor::CENTER,
                Transform::from_xyz(0.0, 0.0, 0.025),
            ));
        })
        .id();

    commands.entity(camera).add_child(callout);
}

pub(super) fn interact_callouts(
    window: Single<&Window, With<bevy::window::PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<FreeFlyCamera>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut callouts: Query<(&StudioCallout, &GlobalTransform, &mut Transform)>,
    mut pending: ResMut<PendingMenuCommand>,
    mut mesh_viewport: ResMut<MeshViewportState>,
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
            mesh_viewport.pointer_click_consumed = true;
            match &callout.action {
                StudioCalloutAction::Command(action) => {
                    pending.0 = Some(action.clone());
                }
                StudioCalloutAction::SelectMode(mode) => {
                    mesh_viewport.mode = *mode;
                    mesh_viewport.selection.clear();
                }
            }
        }
    }
}
