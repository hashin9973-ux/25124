use std::{
    collections::HashSet,
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use avian3d::prelude::Gravity;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::{EditorScreen, MenuAction, PendingMenuCommand};
use crate::{
    config::{BodyType, RuntimeConfig},
    persistence::{WorldSaveTarget, WorldStateFile},
};

#[derive(Resource, Clone, Debug)]
pub struct EditorProjectRoot(pub PathBuf);

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorldKind {
    PhysicsSandbox,
    CharacterStudio,
    Empty,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorldCatalogEntry {
    pub id: String,
    pub name: String,
    pub kind: WorldKind,
    pub manifest: String,
    pub data: String,
    pub assets: String,
    pub outputs: String,
}

#[derive(Resource, Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorldCatalog {
    pub schema_version: u32,
    pub worlds: Vec<WorldCatalogEntry>,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct ActiveWorld {
    pub id: Option<String>,
    pub kind: Option<WorldKind>,
    pub outputs_dir: Option<PathBuf>,
}

#[derive(Resource, Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterStudioWorkspace {
    pub schema_version: u32,
    pub display_name: String,
    pub units: String,
    pub species_index: String,
    pub actions_index: String,
    pub tool_catalog: String,
    pub output_directories: Vec<String>,
}

impl CharacterStudioWorkspace {
    fn load(data_dir: &Path) -> Result<Self> {
        let path = data_dir.join("editor/workspace.json");
        let contents = fs::read_to_string(&path)
            .with_context(|| format!("could not read studio workspace '{}'", path.display()))?;
        let workspace: Self = serde_json::from_str(&contents)
            .with_context(|| format!("invalid studio workspace '{}'", path.display()))?;
        ensure!(
            workspace.schema_version == 1,
            "unsupported character studio workspace schema {}",
            workspace.schema_version
        );
        ensure!(
            !workspace.display_name.trim().is_empty(),
            "character studio workspace display_name cannot be empty"
        );

        for relative in [
            &workspace.species_index,
            &workspace.actions_index,
            &workspace.tool_catalog,
        ] {
            let resolved = safe_project_path(data_dir, relative)?;
            ensure!(
                resolved.is_file(),
                "character studio workspace reference '{}' does not exist",
                resolved.display()
            );
        }
        Ok(workspace)
    }
}

pub(super) fn load_world_catalog(
    root: Res<EditorProjectRoot>,
    mut catalog: ResMut<WorldCatalog>,
) {
    let path = root.0.join("worlds/catalog.json");
    match read_catalog(&path) {
        Ok(loaded) => {
            *catalog = loaded;
            info!(
                "Loaded {} world entries from '{}'",
                catalog.worlds.len(),
                path.display()
            );
        }
        Err(error) => {
            error!("Could not load world catalog '{}': {error:#}", path.display());
        }
    }
}

fn read_catalog(path: &Path) -> Result<WorldCatalog> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("could not read world catalog '{}'", path.display()))?;
    let catalog: WorldCatalog = serde_json::from_str(&contents)
        .with_context(|| format!("invalid world catalog '{}'", path.display()))?;
    ensure!(catalog.schema_version == 1, "unsupported world catalog schema");

    let mut ids = HashSet::new();
    for world in &catalog.worlds {
        ensure!(!world.id.trim().is_empty(), "world catalog ids cannot be empty");
        ensure!(
            ids.insert(world.id.as_str()),
            "duplicate world catalog id '{}'",
            world.id
        );
        for path in [&world.manifest, &world.data, &world.assets, &world.outputs] {
            validate_relative_path(path)?;
        }
    }
    Ok(catalog)
}

fn validate_relative_path(path: &str) -> Result<()> {
    let candidate = Path::new(path);
    ensure!(!candidate.is_absolute(), "catalog paths must be relative");
    ensure!(
        candidate
            .components()
            .all(|component| matches!(component, Component::Normal(_))),
        "catalog paths cannot contain root, parent, or current-directory components"
    );
    ensure!(!path.trim().is_empty(), "catalog paths cannot be empty");
    Ok(())
}

fn safe_project_path(root: &Path, relative: &str) -> Result<PathBuf> {
    validate_relative_path(relative)?;
    Ok(root.join(relative))
}

pub(super) fn process_menu_command(
    mut pending: ResMut<PendingMenuCommand>,
    root: Res<EditorProjectRoot>,
    mut catalog: ResMut<WorldCatalog>,
    mut runtime_config: ResMut<RuntimeConfig>,
    mut state_file: ResMut<WorldStateFile>,
    mut save_target: ResMut<WorldSaveTarget>,
    mut gravity: ResMut<Gravity>,
    mut active_world: ResMut<ActiveWorld>,
    mut studio_workspace: ResMut<CharacterStudioWorkspace>,
    mut next_screen: ResMut<NextState<EditorScreen>>,
) {
    let Some(action) = pending.0.take() else {
        return;
    };

    let result = match action {
        MenuAction::Disabled => Ok(()),
        MenuAction::OpenWorldBrowser => {
            next_screen.set(EditorScreen::WorldBrowser);
            Ok(())
        }
        MenuAction::BackToMainMenu => {
            next_screen.set(EditorScreen::MainMenu);
            Ok(())
        }
        MenuAction::LoadWorld(id) => {
            let selected = catalog.worlds.iter().find(|world| world.id == id).cloned();
            match selected {
                Some(world) => open_world(
                    &root.0,
                    &world,
                    &mut runtime_config,
                    &mut state_file,
                    &mut save_target,
                    &mut gravity,
                    &mut active_world,
                    &mut studio_workspace,
                    &mut next_screen,
                ),
                None => bail!("world id '{id}' is not present in the catalog"),
            }
        }
        MenuAction::CreateWorld => {
            create_empty_world(&root.0, &mut catalog).and_then(|world| {
                open_world(
                    &root.0,
                    &world,
                    &mut runtime_config,
                    &mut state_file,
                    &mut save_target,
                    &mut gravity,
                    &mut active_world,
                    &mut studio_workspace,
                    &mut next_screen,
                )
            })
        }
    };

    if let Err(error) = result {
        error!("Editor command failed: {error:#}");
    }
}

fn open_world(
    project_root: &Path,
    entry: &WorldCatalogEntry,
    runtime_config: &mut RuntimeConfig,
    state_file: &mut WorldStateFile,
    save_target: &mut WorldSaveTarget,
    gravity: &mut Gravity,
    active_world: &mut ActiveWorld,
    studio_workspace: &mut CharacterStudioWorkspace,
    next_screen: &mut NextState<EditorScreen>,
) -> Result<()> {
    let manifest = safe_project_path(project_root, &entry.manifest)?;
    let data_dir = safe_project_path(project_root, &entry.data)?;
    let assets_dir = safe_project_path(project_root, &entry.assets)?;
    let outputs_dir = safe_project_path(project_root, &entry.outputs)?;

    ensure!(manifest.is_file(), "world manifest '{}' does not exist", manifest.display());
    ensure!(data_dir.is_dir(), "world data directory '{}' does not exist", data_dir.display());
    ensure!(assets_dir.is_dir(), "world asset directory '{}' does not exist", assets_dir.display());
    ensure!(outputs_dir.is_dir(), "world outputs directory '{}' does not exist", outputs_dir.display());

    let loaded = RuntimeConfig::load(
        project_root.to_owned(),
        data_dir,
        assets_dir,
        manifest,
    )
    .with_context(|| format!("could not load world '{}'", entry.id))?;
    let loaded_state =
        WorldStateFile::load_or_default(&loaded.paths.world_state, &loaded.world.id)?;

    if matches!(entry.kind, WorldKind::CharacterStudio) {
        *studio_workspace = CharacterStudioWorkspace::load(&loaded.paths.data_dir)?;
        for directory in &studio_workspace.output_directories {
            let output = safe_project_path(&outputs_dir, directory)?;
            fs::create_dir_all(&output).with_context(|| {
                format!("could not create workspace output '{}'", output.display())
            })?;
        }
    } else {
        *studio_workspace = CharacterStudioWorkspace::default();
    }

    for id in loaded_state.bodies.keys() {
        let instance = loaded.world.objects.iter().find(|instance| instance.id == *id);
        match instance {
            None => warn!(
                "Save for world '{}' contains unresolved object '{}'; retaining its state",
                loaded.world.id, id
            ),
            Some(instance) => {
                let definition = loaded
                    .components
                    .definitions
                    .get(&instance.definition)
                    .expect("world references are validated before startup");
                if matches!(definition.body_type, BodyType::Static) {
                    warn!("Ignoring runtime state for static world object '{}'", id);
                }
            }
        }
    }

    let selected_gravity = loaded
        .world
        .gravity_m_s2
        .unwrap_or(loaded.engine.physics.gravity_m_s2);
    *gravity = Gravity(Vec3::from_array(selected_gravity));
    *save_target = WorldSaveTarget {
        world_id: loaded.world.id.clone(),
        state_path: loaded.paths.world_state.clone(),
    };
    *state_file = loaded_state;
    *runtime_config = loaded;
    active_world.id = Some(entry.id.clone());
    active_world.kind = Some(entry.kind);
    active_world.outputs_dir = Some(outputs_dir);
    next_screen.set(EditorScreen::InWorld);
    info!("Entering world '{}' ({:?})", entry.name, entry.kind);
    Ok(())
}

fn create_empty_world(
    project_root: &Path,
    catalog: &mut WorldCatalog,
) -> Result<WorldCatalogEntry> {
    let mut index = 1_u32;
    let id = loop {
        let candidate = format!("world_{index:03}");
        if !catalog.worlds.iter().any(|world| world.id == candidate) {
            break candidate;
        }
        index = index.saturating_add(1);
    };

    let world_name = format!("New World {index}");
    let world_root = project_root.join("worlds").join(&id);
    let data_dir = world_root.join("data");
    let assets_dir = world_root.join("assets");
    let outputs_dir = world_root.join("outputs");

    for directory in [
        assets_dir.join("models"),
        assets_dir.join("materials"),
        assets_dir.join("textures"),
        assets_dir.join("environments"),
        data_dir.clone(),
        outputs_dir.join("species"),
        outputs_dir.join("models"),
        outputs_dir.join("actions"),
        outputs_dir.join("previews"),
        outputs_dir.join("autosaves"),
    ] {
        fs::create_dir_all(&directory).with_context(|| {
            format!("could not create new-world directory '{}'", directory.display())
        })?;
    }

    let source_data = project_root.join("data");
    for file in ["engine.json", "materials.json", "components.json"] {
        fs::copy(source_data.join(file), data_dir.join(file))
            .with_context(|| format!("could not copy base configuration '{file}'"))?;
    }

    let manifest = serde_json::json!({
        "schema_version": 1,
        "id": id,
        "name": world_name,
        "gravity_m_s2": null,
        "objects": []
    });
    write_json(&world_root.join("world.json"), &manifest)?;

    let entry = WorldCatalogEntry {
        id: id.clone(),
        name: world_name,
        kind: WorldKind::Empty,
        manifest: format!("worlds/{id}/world.json"),
        data: format!("worlds/{id}/data"),
        assets: format!("worlds/{id}/assets"),
        outputs: format!("worlds/{id}/outputs"),
    };
    catalog.worlds.push(entry.clone());
    write_json(&project_root.join("worlds/catalog.json"), catalog)?;
    Ok(entry)
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let temp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(value)?;
    fs::write(&temp, bytes)
        .with_context(|| format!("could not write temporary JSON '{}'", temp.display()))?;
    if path.exists() {
        fs::remove_file(path)
            .with_context(|| format!("could not replace JSON '{}'", path.display()))?;
    }
    fs::rename(&temp, path)
        .with_context(|| format!("could not commit JSON '{}'", path.display()))?;
    Ok(())
}
