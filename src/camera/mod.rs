mod free_fly;

use bevy::prelude::*;

use crate::editor::EditorScreen;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(EditorScreen::InWorld), free_fly::spawn_camera)
            .add_systems(
                Update,
                free_fly::move_camera.run_if(in_state(EditorScreen::InWorld)),
            )
            .add_systems(OnExit(EditorScreen::InWorld), free_fly::despawn_camera);
    }
}
