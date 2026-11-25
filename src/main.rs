mod camera;
mod config;
mod engine;
mod editor;
mod persistence;
mod modeling;
mod physics;
mod rendering;
mod world;

use anyhow::Result;
use bevy::{asset::AssetPlugin, prelude::*};

use crate::{
    config::RuntimeConfig,
    editor::EditorPlugin,
    engine::EnginePlugin,
    persistence::WorldStateFile,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("Fatal startup error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let Some(config) = RuntimeConfig::from_environment()? else {
        return Ok(());
    };
    // The menu does not bind the runtime to the bootstrap world's save.
    // A selected catalog world loads its own state before entering InWorld.
    let world_state = WorldStateFile::new(config.world.id.clone());
    let asset_root = config.paths.root_dir.to_string_lossy().into_owned();

    App::new()
        .insert_resource(config)
        .insert_resource(world_state)
        .add_plugins(DefaultPlugins.set(AssetPlugin {
            file_path: asset_root,
            ..default()
        }))
        .add_plugins((EditorPlugin, EnginePlugin))
        .run();

    Ok(())
}

