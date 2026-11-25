mod spawn;

use bevy::prelude::*;

use crate::editor::EditorScreen;

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct WorldInstanceEntity;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(EditorScreen::InWorld), spawn::spawn_world)
            .add_systems(OnExit(EditorScreen::InWorld), despawn_world);
    }
}

fn despawn_world(
    mut commands: Commands,
    entities: Query<Entity, With<WorldInstanceEntity>>,
) {
    for entity in &entities {
        commands.entity(entity).despawn();
    }
}
