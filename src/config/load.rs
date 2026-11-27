use std::{
    collections::{BTreeMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};

use super::{
    schema::{
        ComponentCatalog, EngineSettings, MaterialCatalog, RegionManifest, WorldManifest,
        WorldObjectInstance,
    },
    validate::validate_all,
};

const REGION_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Debug)]
pub struct ProjectPaths {
    pub project_root: PathBuf,
    pub data_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub world_state: PathBuf,
}

/// A validated spatial index entry. The chunk JSON itself is deliberately not loaded here.
#[derive(Clone, Debug)]
pub struct ChunkSource {
    pub id: String,
    pub region_id: String,
    pub region_coordinate: [i32; 2],
    pub coordinate: [i32; 3],
    pub dimensions: [u32; 3],
    pub voxel_size_m: f32,
    pub path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct ObjectChunkSource {
    pub id: String,
    pub coordinate: [i32; 3],
    pub objects: Vec<WorldObjectInstance>,
}

#[derive(bevy::prelude::Resource, Clone, Debug)]
pub struct RuntimeConfig {
    pub engine: EngineSettings,
    pub materials: MaterialCatalog,
    pub components: ComponentCatalog,
    pub world: WorldManifest,
    pub chunks: Vec<ChunkSource>,
    pub object_chunks: Vec<ObjectChunkSource>,
    pub paths: ProjectPaths,
    /// True when no explicit --world argument was supplied.
    pub start_in_library: bool,
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
        let start_in_library = world_override.is_none();
        let mut config = if let Some(path) = world_override {
            Self::load(data_dir, assets_dir, resolve_path(&root, path))?
        } else {
            let library = super::library::WorldLibrary::discover(&root)?;
            let mut selected = None;
            let mut load_errors = Vec::new();

            for entry in library.entries {
                match Self::load(
                    data_dir.clone(),
                    assets_dir.clone(),
                    entry.manifest_path.clone(),
                ) {
                    Ok(config) => {
                        selected = Some(config);
                        break;
                    }
                    Err(error) => {
                        load_errors.push(format!("{}: {error:#}", entry.manifest_path.display()))
                    }
                }
            }

            if !load_errors.is_empty() {
                bevy::log::warn!(
                    "No valid world could bootstrap the library. Candidates failed: {}",
                    load_errors.join("; ")
                );
            }

            match selected {
                Some(config) => config,
                None => Self::load_library_bootstrap(data_dir, assets_dir)?,
            }
        };

        config.paths.project_root = root;
        config.start_in_library = start_in_library;
        Ok(Some(config))
    }

    /// Loads shared engine/content settings without requiring a world manifest.
    /// This keeps the world library available when the project has no worlds yet.
    fn load_library_bootstrap(data_dir: PathBuf, assets_dir: PathBuf) -> Result<Self> {
        let engine: EngineSettings = read_json(&data_dir.join("engine.json"))?;
        let materials: MaterialCatalog = read_json(&data_dir.join("materials.json"))?;
        let components: ComponentCatalog = read_json(&data_dir.join("components.json"))?;
        let world = WorldManifest {
            schema_version: 1,
            id: "__world_library__".to_owned(),
            name: "World Library".to_owned(),
            description: String::new(),
            preview_asset: None,
            metadata: BTreeMap::new(),
            gravity_m_s2: None,
            regions: Vec::new(),
            objects: Vec::new(),
        };
        let chunks = Vec::new();
        validate_all(&engine, &materials, &components, &world, &chunks)
            .context("shared project configuration validation failed")?;
        let project_root = data_dir
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();

        Ok(Self {
            engine,
            materials,
            components,
            world,
            chunks,
            object_chunks: Vec::new(),
            paths: ProjectPaths {
                project_root: project_root.clone(),
                data_dir,
                assets_dir,
                world_state: project_root
                    .join("worlds")
                    .join(".library-bootstrap-state.json"),
            },
            start_in_library: true,
        })
    }

    pub fn load(
        data_dir: PathBuf,
        assets_dir: PathBuf,
        world_manifest_path: PathBuf,
    ) -> Result<Self> {
        let engine: EngineSettings = read_json(&data_dir.join("engine.json"))?;
        let materials: MaterialCatalog = read_json(&data_dir.join("materials.json"))?;
        let components: ComponentCatalog = read_json(&data_dir.join("components.json"))?;
        let world: WorldManifest = read_json(&world_manifest_path)?;
        bevy::log::debug!(
            "Loaded world '{}' presentation metadata: description={} chars, preview_asset={}, metadata_fields={}",
            world.name,
            world.description.chars().count(),
            world.preview_asset.is_some(),
            world.metadata.len()
        );
        let chunks = load_regions(&world_manifest_path, &world)?;
        let object_chunks =
            build_object_chunks(&world.objects, engine.streaming.object_chunk_size_m)?;

        validate_all(&engine, &materials, &components, &world, &chunks)
            .context("configuration validation failed")?;

        let world_state_path = world_manifest_path.with_file_name("state.json");
        let project_root = data_dir
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();

        Ok(Self {
            engine,
            materials,
            components,
            world,
            chunks,
            object_chunks,
            paths: ProjectPaths {
                project_root,
                data_dir,
                assets_dir,
                world_state: world_state_path,
            },
            start_in_library: false,
        })
    }
}

pub(crate) fn object_chunk_coordinate(position_m: [f32; 3], chunk_size_m: f32) -> Result<[i32; 3]> {
    ensure!(
        chunk_size_m.is_finite() && chunk_size_m > 0.0,
        "object chunk size must be positive and finite"
    );
    let mut coordinate = [0_i32; 3];
    for axis in 0..3 {
        ensure!(
            position_m[axis].is_finite(),
            "world object position must be finite"
        );
        let chunk_coordinate = (position_m[axis] / chunk_size_m).floor();
        ensure!(
            chunk_coordinate >= i32::MIN as f32 && chunk_coordinate <= i32::MAX as f32,
            "world object position exceeds the object chunk coordinate range"
        );
        coordinate[axis] = chunk_coordinate as i32;
    }
    Ok(coordinate)
}

fn build_object_chunks(
    objects: &[WorldObjectInstance],
    chunk_size_m: f32,
) -> Result<Vec<ObjectChunkSource>> {
    let mut groups = BTreeMap::<[i32; 3], Vec<WorldObjectInstance>>::new();
    for object in objects {
        let coordinate = object_chunk_coordinate(object.position_m, chunk_size_m)
            .with_context(|| format!("could not spatially index world object '{}'", object.id))?;
        groups.entry(coordinate).or_default().push(object.clone());
    }

    Ok(groups
        .into_iter()
        .map(|(coordinate, objects)| ObjectChunkSource {
            id: format!(
                "objects_{}_{}_{}",
                coordinate[0], coordinate[1], coordinate[2]
            ),
            coordinate,
            objects,
        })
        .collect())
}

fn load_regions(world_path: &Path, world: &WorldManifest) -> Result<Vec<ChunkSource>> {
    let world_directory = world_path
        .parent()
        .context("world manifest path has no parent directory")?;
    let mut region_ids = HashSet::new();
    let mut chunk_ids = HashSet::new();
    let mut chunk_coordinates = HashSet::new();
    let mut chunks = Vec::new();

    for region_reference in &world.regions {
        validate_relative_data_path(region_reference)
            .with_context(|| format!("invalid region path '{region_reference}'"))?;
        let region_path = world_directory.join(region_reference);
        let region: RegionManifest = read_json(&region_path)?;
        ensure!(
            region.schema_version == REGION_SCHEMA_VERSION,
            "unsupported schema version {} in region '{}'",
            region.schema_version,
            region.id
        );
        ensure!(!region.id.trim().is_empty(), "region id cannot be empty");
        ensure!(
            region_ids.insert(region.id.clone()),
            "duplicate region id '{}'",
            region.id
        );

        let region_directory = region_path
            .parent()
            .context("region manifest path has no parent directory")?;
        let mut region_chunk_ids = HashSet::new();
        for reference in &region.chunks {
            ensure!(!reference.id.trim().is_empty(), "chunk id cannot be empty");
            ensure!(
                region_chunk_ids.insert(reference.id.as_str()),
                "region '{}' contains duplicate chunk id '{}'",
                region.id,
                reference.id
            );
            validate_relative_data_path(&reference.path).with_context(|| {
                format!(
                    "invalid chunk path '{}' in region '{}'",
                    reference.path, region.id
                )
            })?;
            ensure!(
                reference
                    .dimensions
                    .iter()
                    .all(|dimension| (1..=32).contains(dimension)),
                "chunk '{}' dimensions must each be in [1, 32]",
                reference.id
            );
            ensure!(
                reference.voxel_size_m.is_finite() && reference.voxel_size_m > 0.0,
                "chunk '{}' voxel_size_m must be positive and finite",
                reference.id
            );
            for axis in 0..3 {
                let cell_origin =
                    reference.coordinate[axis] as i64 * reference.dimensions[axis] as i64;
                let cell_end = cell_origin + reference.dimensions[axis] as i64 - 1;
                ensure!(
                    cell_origin >= i32::MIN as i64 && cell_end <= i32::MAX as i64,
                    "chunk '{}' coordinate on axis {axis} exceeds the global voxel index range",
                    reference.id
                );

                let world_extent = reference.dimensions[axis] as f32 * reference.voxel_size_m;
                let world_min = reference.coordinate[axis] as f32 * world_extent;
                ensure!(
                    world_extent.is_finite()
                        && world_extent > 0.0
                        && world_min.is_finite()
                        && (world_min + world_extent).is_finite(),
                    "chunk '{}' world bounds on axis {axis} are not finite",
                    reference.id
                );
            }

            ensure!(
                chunk_ids.insert(reference.id.clone()),
                "duplicate chunk id '{}'",
                reference.id
            );
            ensure!(
                chunk_coordinates.insert(reference.coordinate),
                "multiple chunks occupy coordinate {:?}",
                reference.coordinate
            );
            chunks.push(ChunkSource {
                id: reference.id.clone(),
                region_id: region.id.clone(),
                region_coordinate: region.coordinate,
                coordinate: reference.coordinate,
                dimensions: reference.dimensions,
                voxel_size_m: reference.voxel_size_m,
                path: region_directory.join(&reference.path),
            });
        }
    }

    validate_uniform_chunk_lattice(&chunks)?;
    Ok(chunks)
}

fn validate_uniform_chunk_lattice(chunks: &[ChunkSource]) -> Result<()> {
    let Some(reference) = chunks.first() else {
        return Ok(());
    };

    for chunk in chunks.iter().skip(1) {
        ensure!(
            chunk.dimensions == reference.dimensions,
            "chunk '{}' dimensions {:?} do not match world grid dimensions {:?}",
            chunk.id,
            chunk.dimensions,
            reference.dimensions
        );
        ensure!(
            (chunk.voxel_size_m - reference.voxel_size_m).abs()
                <= f32::EPSILON * reference.voxel_size_m.max(1.0),
            "chunk '{}' voxel size {} does not match world grid voxel size {}",
            chunk.id,
            chunk.voxel_size_m,
            reference.voxel_size_m
        );
    }
    Ok(())
}

fn validate_relative_data_path(path: &str) -> Result<()> {
    ensure!(!path.trim().is_empty(), "path is empty");
    ensure!(
        !path.starts_with('/') && !path.contains('\\') && !path.contains(':'),
        "paths must be relative, use forward slashes, and contain no URI or drive prefix"
    );
    for component in path.split('/') {
        ensure!(
            !component.is_empty() && component != "." && component != "..",
            "paths cannot contain empty, current-directory, or parent-directory components"
        );
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::config::RuntimeConfig;

    #[test]
    fn library_bootstrap_loads_shared_settings_without_a_world_manifest() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let config = RuntimeConfig::load_library_bootstrap(
            root.join("data"),
            root.join("assets"),
        )
        .expect("shared project configuration should validate");

        assert_eq!(config.world.id, "__world_library__");
        assert!(config.world.objects.is_empty());
        assert!(config.chunks.is_empty());
        assert!(config.object_chunks.is_empty());
        assert!(config.start_in_library);
        assert!(
            config
                .paths
                .world_state
                .ends_with("worlds/.library-bootstrap-state.json")
        );
    }

    #[test]
    fn checked_in_configuration_loads_region_indexes_without_loading_chunk_bodies() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let config = RuntimeConfig::load(
            root.join("data"),
            root.join("assets"),
            root.join("worlds/sandbox/world.json"),
        )
        .expect("sample configuration should validate");

        assert_eq!(config.world.id, "sandbox");
        assert_eq!(config.world.objects.len(), 4);
        assert_eq!(config.chunks.len(), 4);
        assert!(config.chunks.iter().all(|source| source.path.is_file()));
        assert_eq!(config.chunks[0].dimensions, [16, 16, 16]);
        assert_eq!(config.chunks[0].voxel_size_m, 1.0);
        assert_eq!(config.object_chunks.len(), 2);
        assert_eq!(config.object_chunks[0].coordinate, [-1, 0, 0]);
        assert_eq!(config.object_chunks[0].objects.len(), 2);
        assert_eq!(config.object_chunks[1].coordinate, [0, 0, 0]);
        assert_eq!(config.object_chunks[1].objects.len(), 2);
    }

    #[test]
    fn object_chunk_coordinates_use_floor_for_negative_world_positions() {
        assert_eq!(
            super::object_chunk_coordinate([-0.01, 0.0, 16.0], 16.0).unwrap(),
            [-1, 0, 1]
        );
        assert_eq!(
            super::object_chunk_coordinate([-16.0, -16.01, 0.0], 16.0).unwrap(),
            [-1, -2, 0]
        );
        assert!(super::object_chunk_coordinate([f32::NAN, 0.0, 0.0], 16.0).is_err());
        assert!(super::object_chunk_coordinate([0.0; 3], 0.0).is_err());
    }

    #[test]
    fn relative_content_paths_reject_parent_traversal() {
        assert!(super::validate_relative_data_path("regions/overworld.json").is_ok());
        assert!(super::validate_relative_data_path("../outside.json").is_err());
        assert!(super::validate_relative_data_path("C:/outside.json").is_err());
        assert!(super::validate_relative_data_path("regions\\overworld.json").is_err());
    }
}
