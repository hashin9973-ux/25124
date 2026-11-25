mod camera;
mod config;
mod engine;
mod persistence;
mod physics;
mod rendering;
mod world;

use anyhow::Result;
use bevy::{asset::AssetPlugin, prelude::*};

use crate::config::RuntimeConfig;
use crate::engine::EnginePlugin;

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

    let asset_root = config.paths.assets_dir.to_string_lossy().into_owned();

    App::new()
        .insert_resource(config)
        .add_plugins(DefaultPlugins.set(AssetPlugin {
            file_path: asset_root,
            ..default()
        }))
        .add_plugins(EnginePlugin)
        .run();

    Ok(())
}
