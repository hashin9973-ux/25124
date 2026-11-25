use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use super::{
    schema::{ComponentCatalog, EngineSettings, MaterialCatalog, WorldManifest},
    validate::validate_all,
};

#[derive(Clone, Debug)]
pub struct ProjectPaths {
    pub root_dir: PathBuf,
    pub data_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub world_manifest: PathBuf,
    pub world_state: PathBuf,
}

#[derive(bevy::prelude::Resource, Clone, Debug)]
pub struct RuntimeConfig {
    pub engine: EngineSettings,
    pub materials: MaterialCatalog,
    pub components: ComponentCatalog,
    pub world: WorldManifest,
    pub paths: ProjectPaths,
}

impl RuntimeConfig {
    pub fn from_environment() -> Result<Option<Self>> {
        let mut root_override = None;
        let mut data_override = None;
        let mut assets_override = None;
        let mut world_override = None;

        let mut args = env::args().skip(1);
        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--help" | "-h" => {
                    print_help();
                    return Ok(None);
                }
                "--root" => root_override = Some(next_path(&mut args, "--root")?),
                "--data" => data_override = Some(next_path(&mut args, "--data")?),
                "--assets" => assets_override = Some(next_path(&mut args, "--assets")?),
                "--world" => world_override = Some(next_path(&mut args, "--world")?),
                unknown => bail!("unknown argument '{unknown}'. Run with --help for usage."),
            }
        }

        let root = match root_override {
            Some(path) => path,
            None => detect_project_root()?,
        };
        let data_dir = resolve_path(
            &root,
            data_override.unwrap_or_else(|| PathBuf::from("data")),
        );
        let assets_dir = resolve_path(
            &root,
            assets_override.unwrap_or_else(|| PathBuf::from("assets")),
        );
        let world_manifest = resolve_path(
            &root,
            world_override.unwrap_or_else(|| PathBuf::from("worlds/sandbox/world.json")),
        );

        Self::load(root, data_dir, assets_dir, world_manifest).map(Some)
    }

    pub fn load(
        root_dir: PathBuf,
        data_dir: PathBuf,
        assets_dir: PathBuf,
        world_manifest_path: PathBuf,
    ) -> Result<Self> {
        let engine: EngineSettings = read_json(&data_dir.join("engine.json"))?;
        let materials: MaterialCatalog = read_json(&data_dir.join("materials.json"))?;
        let components: ComponentCatalog = read_json(&data_dir.join("components.json"))?;
        let world: WorldManifest = read_json(&world_manifest_path)?;

        validate_all(&engine, &materials, &components, &world)
            .context("configuration validation failed")?;

        let world_state_path = world_manifest_path.with_file_name("state.json");

        Ok(Self {
            engine,
            materials,
            components,
            world,
            paths: ProjectPaths {
                root_dir,
                data_dir,
                assets_dir,
                world_manifest: world_manifest_path,
                world_state: world_state_path,
            },
        })
    }
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("could not read configuration file '{}'", path.display()))?;

    serde_json::from_str(&contents)
        .with_context(|| format!("invalid JSON configuration in '{}'", path.display()))
}

fn next_path(args: &mut impl Iterator<Item = String>, option: &str) -> Result<PathBuf> {
    args.next()
        .map(PathBuf::from)
        .with_context(|| format!("option {option} requires a path"))
}

fn resolve_path(root: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

fn detect_project_root() -> Result<PathBuf> {
    if let Some(executable_dir) = env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .filter(|path| path.join("data").is_dir())
    {
        return Ok(executable_dir);
    }

    env::current_dir().context("could not determine the project root")
}

fn print_help() {
    println!(
        "Physics Sandbox\n\
         Usage: physics_sandbox [--root PATH] [--data PATH] [--assets PATH] [--world PATH]\n\
         Paths may be absolute or relative to --root.\n"
    );
}
