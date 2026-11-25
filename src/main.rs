mod camera;
mod config;
mod engine;
mod persistence;
mod physics;
mod rendering;
mod world;

use anyhow::Result;
use bevy::{asset::AssetPlugin, prelude::*};

use crate::{
    config::{BodyType, RuntimeConfig},
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
    let world_state = WorldStateFile::load_or_default(&config.paths.world_state, &config.world.id)?;
    report_saved_state_compatibility(&config, &world_state);
    let asset_root = config.paths.assets_dir.to_string_lossy().into_owned();

    App::new()
        .insert_resource(config)
        .insert_resource(world_state)
        .add_plugins(DefaultPlugins.set(AssetPlugin {
            file_path: asset_root,
            ..default()
        }))
        .add_plugins(EnginePlugin)
        .run();

    Ok(())
}

fn report_saved_state_compatibility(config: &RuntimeConfig, world_state: &WorldStateFile) {
    for id in world_state.bodies.keys() {
        let Some(instance) = config
            .world
            .objects
            .iter()
            .find(|instance| instance.id == *id)
        else {
            eprintln!(
                "Warning: save contains unresolved object id '{id}'. Its state is preserved for content that may be loaded later."
            );
            continue;
        };

        let definition = config
            .components
            .definitions
            .get(&instance.definition)
            .expect("world definitions are validated before app startup");
        if matches!(definition.body_type, BodyType::Static) {
            eprintln!(
                "Warning: ignoring persisted state for static object '{id}'; its transform is authored by the world manifest. The stale record will be removed on the next save."
            );
        }
    }
}
