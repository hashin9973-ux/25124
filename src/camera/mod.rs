mod free_fly;

use bevy::prelude::*;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, free_fly::spawn_camera)
            .add_systems(Update, free_fly::move_camera);
    }
}
