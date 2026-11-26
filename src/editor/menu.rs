use bevy::prelude::*;
use bevy::sprite::Anchor;

use super::{
    MenuAction, MenuSceneDirty, PendingMenuCommand, WORLD_PAGE_SIZE, WorldBrowserPage,
    WorldCatalog,
    water::{RippleField, spawn_water},
};

#[derive(Component)]
pub(super) struct MenuScene;

#[derive(Component)]
pub(super) struct MenuCamera;

#[derive(Component)]
pub(super) struct MenuCallout {
    action: MenuAction,
    label_position: Vec3,
    hit_extent: Vec2,
}

pub(super) fn spawn_main_menu(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut ripples: ResMut<RippleField>,
) {
    *ripples = RippleField::default();
    spawn_base_scene(&mut commands, &mut meshes, &mut materials);

    spawn_heading(
        &mut commands,
        "PHYSICS SANDBOX",
        Vec3::new(0.0, 5.1, -7.0),
        camera_rotation(),
        64.0,
    );

    let black = materials.add(Color::BLACK);
    spawn_callout(
        &mut commands,
        &mut meshes,
        &black,
        "WORLDS",
        Vec3::new(-7.0, 0.0, -1.0),
        MenuAction::OpenWorldBrowser,
        camera_rotation(),
    );
    spawn_callout(
        &mut commands,
        &mut meshes,
        &black,
        "MODS",
        Vec3::new(0.0, 0.0, -1.0),
        MenuAction::Disabled,
        camera_rotation(),
    );
    spawn_callout(
        &mut commands,
        &mut meshes,
        &black,
        "SETTINGS",
        Vec3::new(7.0, 0.0, -1.0),
        MenuAction::Disabled,
        camera_rotation(),
    );
}

pub(super) fn spawn_world_browser(
    mut commands: Commands,
    catalog: Res<WorldCatalog>,
    page: Res<WorldBrowserPage>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut ripples: ResMut<RippleField>,
) {
    *ripples = RippleField::default();
    build_world_browser_scene(&mut commands, &catalog, page.0, &mut meshes, &mut materials);
}

#[allow(clippy::too_many_arguments)] // Bevy injects separate resources and a scene query into this system.
pub(super) fn refresh_world_browser(
    mut commands: Commands,
    catalog: Res<WorldCatalog>,
    page: Res<WorldBrowserPage>,
    mut scene_dirty: ResMut<MenuSceneDirty>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut ripples: ResMut<RippleField>,
    existing_scene: Query<Entity, With<MenuScene>>,
) {
    if !scene_dirty.0 {
        return;
    }

    scene_dirty.0 = false;
    for entity in &existing_scene {
        commands.entity(entity).despawn();
    }
    *ripples = RippleField::default();
    build_world_browser_scene(&mut commands, &catalog, page.0, &mut meshes, &mut materials);
}

fn build_world_browser_scene(
    commands: &mut Commands,
    catalog: &WorldCatalog,
    page_index: usize,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    spawn_base_scene(commands, meshes, materials);

    spawn_heading(
        commands,
        "SELECT A WORLD",
        Vec3::new(0.0, 5.3, -8.0),
        camera_rotation(),
        58.0,
    );

    let black = materials.add(Color::BLACK);
    spawn_callout(
        commands,
        meshes,
        &black,
        "BACK",
        Vec3::new(-9.0, 0.0, 8.0),
        MenuAction::BackToMainMenu,
        camera_rotation(),
    );

    let page_count = catalog.worlds.len().div_ceil(WORLD_PAGE_SIZE).max(1);
    let selected_page = page_index.min(page_count - 1);
    let first_world = selected_page * WORLD_PAGE_SIZE;
    for (row, entry) in catalog
        .worlds
        .iter()
        .skip(first_world)
        .take(WORLD_PAGE_SIZE)
        .enumerate()
    {
        let z = 4.0 - row as f32 * 2.5;
        spawn_callout(
            commands,
            meshes,
            &black,
            &entry.name.to_uppercase(),
            Vec3::new(0.0, 0.0, z),
            MenuAction::LoadWorld(entry.id.clone()),
            camera_rotation(),
        );
    }

    if page_count > 1 {
        spawn_callout(
            commands,
            meshes,
            &black,
            "PREVIOUS",
            Vec3::new(-7.0, 0.0, -8.5),
            MenuAction::PreviousWorldPage,
            camera_rotation(),
        );
        spawn_callout(
            commands,
            meshes,
            &black,
            &format!("PAGE {} OF {}", selected_page + 1, page_count),
            Vec3::new(0.0, 0.0, -8.5),
            MenuAction::Disabled,
            camera_rotation(),
        );
        spawn_callout(
            commands,
            meshes,
            &black,
            "NEXT",
            Vec3::new(7.0, 0.0, -8.5),
            MenuAction::NextWorldPage,
            camera_rotation(),
        );
    }

    spawn_callout(
        commands,
        meshes,
        &black,
        "CREATE WORLD",
        Vec3::new(9.0, 0.0, -2.0),
        MenuAction::CreateWorld,
        camera_rotation(),
    );
}

fn spawn_base_scene(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    commands.spawn((
        Name::new("Main menu camera"),
        MenuScene,
        MenuCamera,
        Camera3d::default(),
        Transform::from_xyz(0.0, 18.0, 27.0).looking_at(Vec3::new(0.0, 0.0, -1.0), Vec3::Y),
    ));
    spawn_water(commands, meshes, materials);
    commands.spawn((
        Name::new("Menu horizon fill"),
        MenuScene,
        Mesh3d(meshes.add(Plane3d::default().mesh().size(180.0, 180.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.012, 0.055, 0.075),
            perceptual_roughness: 0.32,
            metallic: 0.1,
            ..default()
        })),
        Transform::from_xyz(0.0, -0.24, 0.0),
    ));
}

fn camera_rotation() -> Quat {
    Transform::from_xyz(0.0, 18.0, 27.0)
        .looking_at(Vec3::new(0.0, 0.0, -1.0), Vec3::Y)
        .rotation
}

fn spawn_heading(
    commands: &mut Commands,
    text: &str,
    position: Vec3,
    rotation: Quat,
    font_size: f32,
) {
    let mut heading = commands.spawn((
        Name::new("Menu title annotation"),
        MenuScene,
        Transform {
            translation: position,
            rotation,
            ..default()
        },
    ));
    heading.with_children(|parent| {
        for offset in [
            Vec3::new(-0.035, 0.0, -0.025),
            Vec3::new(0.035, 0.0, -0.025),
            Vec3::new(0.0, -0.035, -0.025),
            Vec3::new(0.0, 0.035, -0.025),
            Vec3::new(-0.025, -0.025, -0.025),
            Vec3::new(0.025, 0.025, -0.025),
        ] {
            parent.spawn((
                Text2d::new(text),
                TextFont::from_font_size(font_size),
                TextColor(Color::BLACK),
                Anchor::CENTER,
                Transform::from_translation(offset),
            ));
        }
        parent.spawn((
            Text2d::new(text),
            TextFont::from_font_size(font_size),
            TextColor(Color::WHITE),
            Anchor::CENTER,
            Transform::from_xyz(0.0, 0.0, 0.02),
        ));
    });
}

fn spawn_callout(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    black_material: &Handle<StandardMaterial>,
    text: &str,
    anchor: Vec3,
    action: MenuAction,
    rotation: Quat,
) {
    let label_position = anchor + Vec3::new(0.0, 2.05, 0.0);
    let line = label_position - (anchor + Vec3::Y * 0.05);
    let line_length = line.length().max(0.05);
    commands.spawn((
        Name::new(format!("{text} annotation stem")),
        MenuScene,
        Mesh3d(meshes.add(Cylinder::new(0.035, line_length))),
        MeshMaterial3d(black_material.clone()),
        Transform {
            translation: (anchor + Vec3::Y * 0.05 + label_position) * 0.5,
            rotation: Quat::from_rotation_arc(Vec3::Y, line.normalize()),
            ..default()
        },
    ));

    let hit_width = (text.chars().count() as f32 * 26.0).clamp(170.0, 540.0);
    let mut callout = commands.spawn((
        Name::new(format!("{text} callout")),
        MenuScene,
        MenuCallout {
            action,
            label_position,
            hit_extent: Vec2::new(hit_width * 0.5, 48.0),
        },
        Transform {
            translation: label_position,
            rotation,
            ..default()
        },
    ));
    callout.with_children(|parent| {
        for offset in [
            Vec3::new(-0.04, 0.0, -0.03),
            Vec3::new(0.04, 0.0, -0.03),
            Vec3::new(0.0, -0.04, -0.03),
            Vec3::new(0.0, 0.04, -0.03),
            Vec3::new(-0.03, -0.03, -0.03),
            Vec3::new(0.03, 0.03, -0.03),
            Vec3::new(0.03, -0.03, -0.03),
            Vec3::new(-0.03, 0.03, -0.03),
        ] {
            parent.spawn((
                Text2d::new(text),
                TextFont::from_font_size(48.0),
                TextColor(Color::BLACK),
                Anchor::CENTER,
                Transform::from_translation(offset),
            ));
        }
        parent.spawn((
            Text2d::new(text),
            TextFont::from_font_size(48.0),
            TextColor(Color::WHITE),
            Anchor::CENTER,
            Transform::from_xyz(0.0, 0.0, 0.025),
        ));
    });
}

pub(super) fn interact_callouts(
    window: Single<&Window, With<bevy::window::PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<MenuCamera>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut callouts: Query<(&MenuCallout, &mut Transform)>,
    mut pending: ResMut<PendingMenuCommand>,
) {
    let cursor = window.cursor_position();
    let (camera, camera_transform) = *camera;

    for (callout, mut transform) in &mut callouts {
        let hovered = cursor.is_some_and(|cursor| {
            camera
                .world_to_viewport(camera_transform, callout.label_position)
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

pub(super) fn despawn_menu_scene(mut commands: Commands, entities: Query<Entity, With<MenuScene>>) {
    for entity in &entities {
        commands.entity(entity).despawn();
    }
}
