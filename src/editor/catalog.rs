use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use avian3d::prelude::Gravity;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    EditorScreen, MenuAction, MenuSceneDirty, PendingMenuCommand, WORLD_PAGE_SIZE,
    WorldBrowserPage,
};
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

#[derive(Resource, Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorldCatalog {
    pub schema_version: u32,
    pub worlds: Vec<WorldCatalogEntry>,
}

impl Default for WorldCatalog {
    fn default() -> Self {
        Self {
            schema_version: 1,
            worlds: Vec::new(),
        }
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct ActiveWorld {
    pub id: Option<String>,
    pub kind: Option<WorldKind>,
    pub outputs_dir: Option<PathBuf>,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct CharacterStudioWorkspace {
    pub schema_version: u32,
    pub id: String,
    pub display_name: String,
    pub units: String,
    pub species_index: String,
    pub actions_index: String,
    pub tool_catalog_path: String,
    pub output_directories: Vec<String>,
    pub species: Vec<Value>,
    pub action_sets: Vec<Value>,
    pub tool_catalog: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceFile {
    schema_version: u32,
    id: String,
    display_name: String,
    units: String,
    species_index: String,
    actions_index: String,
    tool_catalog: String,
    output_directories: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpeciesIndexFile {
    schema_version: u32,
    species: Vec<SpeciesIndexEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpeciesIndexEntry {
    id: String,
    name: String,
    file: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ActionIndexFile {
    schema_version: u32,
    action_sets: Vec<ActionIndexEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ActionIndexEntry {
    id: String,
    species: String,
    file: String,
}

impl CharacterStudioWorkspace {
    fn load(project_root: &Path, data_dir: &Path, expected_id: &str) -> Result<Self> {
        let workspace_path = data_dir.join("editor/workspace.json");
        let workspace: WorkspaceFile = read_json(&workspace_path)?;
        ensure!(
            workspace.schema_version == 1,
            "unsupported character studio workspace schema {}",
            workspace.schema_version
        );
        ensure!(
            workspace.id == expected_id,
            "workspace id '{}' does not match catalog world id '{}'",
            workspace.id,
            expected_id
        );
        ensure!(
            !workspace.display_name.trim().is_empty(),
            "character studio workspace display_name cannot be empty"
        );
        ensure!(
            !workspace.units.trim().is_empty(),
            "character studio workspace units cannot be empty"
        );

        let species_index_path = safe_project_path(data_dir, &workspace.species_index)?;
        let actions_index_path = safe_project_path(data_dir, &workspace.actions_index)?;
        let tool_catalog_path = safe_project_path(data_dir, &workspace.tool_catalog)?;
        let species_index: SpeciesIndexFile = read_json(&species_index_path)?;
        let actions_index: ActionIndexFile = read_json(&actions_index_path)?;
        ensure!(
            species_index.schema_version == 1,
            "unsupported species index schema in '{}'",
            species_index_path.display()
        );
        ensure!(
            actions_index.schema_version == 1,
            "unsupported action index schema in '{}'",
            actions_index_path.display()
        );

        let tool_catalog: Value = read_json(&tool_catalog_path)?;
        ensure!(
            tool_catalog.get("schema_version").and_then(Value::as_u64) == Some(1),
            "unsupported editor tool catalog schema in '{}'",
            tool_catalog_path.display()
        );

        let mut species_ids = HashSet::new();
        let mut species_documents = Vec::with_capacity(species_index.species.len());
        let mut rig_ids = HashMap::<String, (HashSet<String>, HashSet<String>)>::new();

        for reference in &species_index.species {
            ensure!(
                !reference.id.trim().is_empty(),
                "species index contains an empty id"
            );
            ensure!(
                species_ids.insert(reference.id.as_str()),
                "species index contains duplicate id '{}'",
                reference.id
            );
            ensure!(
                !reference.name.trim().is_empty(),
                "species '{}' has an empty display name",
                reference.id
            );

            let species_path = safe_project_path(project_root, &reference.file)?;
            let species: Value = read_json(&species_path)?;
            validate_document_id(&species, &reference.id, &species_path)?;

            let rig_reference = species
                .get("rig")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow::anyhow!("species '{}' is missing its rig path", reference.id))?;
            let rig_path = safe_project_path(project_root, rig_reference)?;
            let rig: Value = read_json(&rig_path)?;
            ensure!(
                rig.get("schema_version").and_then(Value::as_u64) == Some(1),
                "unsupported rig schema in '{}'",
                rig_path.display()
            );
            ensure!(
                rig.get("species").and_then(Value::as_str) == Some(reference.id.as_str()),
                "rig '{}' does not belong to species '{}'",
                rig_path.display(),
                reference.id
            );

            let segment_ids = collect_ids(&rig, "segments", &rig_path)?;
            let joint_ids = collect_ids(&rig, "joints", &rig_path)?;
            rig_ids.insert(reference.id.clone(), (segment_ids, joint_ids));
            species_documents.push(species);
        }

        let mut action_set_ids = HashSet::new();
        let mut action_documents = Vec::with_capacity(actions_index.action_sets.len());
        for reference in &actions_index.action_sets {
            ensure!(
                action_set_ids.insert(reference.id.as_str()),
                "action index contains duplicate id '{}'",
                reference.id
            );
            ensure!(
                species_ids.contains(reference.species.as_str()),
                "action set '{}' references unknown species '{}'",
                reference.id,
                reference.species
            );

            let action_path = safe_project_path(project_root, &reference.file)?;
            let action_set: Value = read_json(&action_path)?;
            validate_document_id(&action_set, &reference.id, &action_path)?;
            ensure!(
                action_set.get("species").and_then(Value::as_str)
                    == Some(reference.species.as_str()),
                "action set '{}' species does not match its index entry",
                reference.id
            );
            let (segments, joints) = rig_ids
                .get(&reference.species)
                .expect("species index and rig ids were loaded together");
            validate_action_set(&action_set, segments, joints, &action_path)?;
            action_documents.push(action_set);
        }

        for relative in &workspace.output_directories {
            validate_relative_path(relative)?;
        }

        Ok(Self {
            schema_version: workspace.schema_version,
            id: workspace.id,
            display_name: workspace.display_name,
            units: workspace.units,
            species_index: workspace.species_index,
            actions_index: workspace.actions_index,
            tool_catalog_path: workspace.tool_catalog,
            output_directories: workspace.output_directories,
            species: species_documents,
            action_sets: action_documents,
            tool_catalog,
        })
    }
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("could not read data file '{}'", path.display()))?;
    serde_json::from_str(&contents)
        .with_context(|| format!("invalid JSON data file '{}'", path.display()))
}

fn validate_document_id(document: &Value, expected: &str, path: &Path) -> Result<()> {
    ensure!(
        document.get("schema_version").and_then(Value::as_u64) == Some(1),
        "unsupported schema in '{}'",
        path.display()
    );
    ensure!(
        document.get("id").and_then(Value::as_str) == Some(expected),
        "document '{}' does not declare expected id '{}'",
        path.display(),
        expected
    );
    Ok(())
}

fn collect_ids(document: &Value, key: &str, path: &Path) -> Result<HashSet<String>> {
    let entries = document
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("rig '{}' has no '{key}' array", path.display()))?;
    let mut ids = HashSet::new();
    for entry in entries {
        let id = entry.get("id").and_then(Value::as_str).ok_or_else(|| {
            anyhow::anyhow!("rig '{}' has a '{key}' entry without an id", path.display())
        })?;
        ensure!(
            !id.trim().is_empty(),
            "rig '{}' has an empty {key} id",
            path.display()
        );
        ensure!(
            ids.insert(id.to_owned()),
            "rig '{}' has duplicate {key} id '{id}'",
            path.display()
        );
    }
    Ok(ids)
}

fn validate_action_set(
    action_set: &Value,
    segments: &HashSet<String>,
    joints: &HashSet<String>,
    path: &Path,
) -> Result<()> {
    let actions = action_set
        .get("actions")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("action set '{}' has no actions array", path.display()))?;
    let mut action_ids = HashSet::new();

    for action in actions {
        let id = action
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("action set '{}' contains an action without an id", path.display()))?;
        ensure!(
            action_ids.insert(id),
            "action set '{}' contains duplicate action id '{id}'",
            path.display()
        );

        for key in ["triggers", "conditions"] {
            if let Some(entries) = action.get(key).and_then(Value::as_array) {
                for entry in entries {
                    let kind = entry.get("kind").and_then(Value::as_str).unwrap_or_default();
                    match kind {
                        "collision" => validate_reference(entry, "segment", segments, "segment", path)?,
                        "surface_contact" => {
                            let segment_list = entry
                                .get("segments")
                                .and_then(Value::as_array)
                                .ok_or_else(|| anyhow::anyhow!("surface contact in '{}' has no segments", path.display()))?;
                            for segment in segment_list {
                                let segment_id = segment.as_str().ok_or_else(|| {
                                    anyhow::anyhow!("surface contact in '{}' has a non-string segment id", path.display())
                                })?;
                                ensure!(
                                    segments.contains(segment_id),
                                    "surface contact in '{}' references unknown segment '{segment_id}'",
                                    path.display()
                                );
                            }
                        }
                        "joint_angle_threshold" => validate_reference(entry, "joint", joints, "joint", path)?,
                        "key_held" => ensure!(
                            entry.get("key").and_then(Value::as_str).is_some(),
                            "key trigger in '{}' has no key",
                            path.display()
                        ),
                        _ => bail!(
                            "unsupported action trigger or condition kind '{kind}' in '{}'",
                            path.display()
                        ),
                    }
                }
            }
        }

        let actuators = action
            .get("actuators")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow::anyhow!("action '{id}' in '{}' has no actuators array", path.display()))?;
        for actuator in actuators {
            validate_reference(actuator, "joint", joints, "actuator joint", path)?;
            let kind = actuator.get("kind").and_then(Value::as_str).unwrap_or_default();
            ensure!(
                matches!(kind, "joint_torque" | "joint_angular_velocity_target"),
                "unsupported actuator kind '{kind}' in '{}'",
                path.display()
            );
            let torque_limit = actuator
                .get("torque_limit_nm")
                .and_then(Value::as_f64)
                .ok_or_else(|| anyhow::anyhow!("actuator in '{}' has no numeric torque limit", path.display()))?;
            ensure!(
                torque_limit.is_finite() && torque_limit > 0.0,
                "actuator torque limit in '{}' must be finite and positive",
                path.display()
            );
        }
    }

    let policy = action_set
        .get("execution_policy")
        .ok_or_else(|| anyhow::anyhow!("action set '{}' has no execution policy", path.display()))?;
    for flag in [
        "allow_impulses",
        "allow_root_teleport",
        "allow_rotation_locks",
        "allow_external_object_push",
    ] {
        ensure!(
            policy.get(flag).and_then(Value::as_bool) == Some(false),
            "action set '{}' must explicitly disable '{flag}'",
            path.display()
        );
    }
    Ok(())
}

fn validate_reference(
    document: &Value,
    key: &str,
    valid_ids: &HashSet<String>,
    label: &str,
    path: &Path,
) -> Result<()> {
    let id = document
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("{label} reference in '{}' is missing '{key}'", path.display()))?;
    ensure!(
        valid_ids.contains(id),
        "unknown {label} '{id}' referenced in '{}'",
        path.display()
    );
    Ok(())
}
pub(super) fn load_world_catalog(
    root: Res<EditorProjectRoot>,
    mut catalog: ResMut<WorldCatalog>,
) {
    let path = root.0.join("worlds/catalog.json");
    match read_catalog(&path) {
        Ok(mut loaded) => {
            let before = loaded.worlds.len();
            loaded.worlds.retain(|world| {
                let paths_exist = root.0.join(&world.manifest).is_file()
                    && root.0.join(&world.data).is_dir()
                    && root.0.join(&world.assets).is_dir()
                    && root.0.join(&world.outputs).is_dir();
                if !paths_exist {
                    warn!(
                        "Skipping unavailable world '{}' because one or more catalog paths are missing",
                        world.id
                    );
                }
                paths_exist
            });
            *catalog = loaded;
            info!(
                "Loaded {} of {} available world entries from '{}'",
                catalog.worlds.len(),
                before,
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
    mut page: ResMut<WorldBrowserPage>,
    mut scene_dirty: ResMut<MenuSceneDirty>,
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
        MenuAction::PreviousWorldPage => {
            let page_count = catalog.worlds.len().div_ceil(WORLD_PAGE_SIZE);
            if page_count > 1 {
                page.0 = (page.0 + page_count - 1) % page_count;
                scene_dirty.0 = true;
            }
            Ok(())
        }
        MenuAction::NextWorldPage => {
            let page_count = catalog.worlds.len().div_ceil(WORLD_PAGE_SIZE);
            if page_count > 1 {
                page.0 = (page.0 + 1) % page_count;
                scene_dirty.0 = true;
            }
            Ok(())
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
        *studio_workspace =
            CharacterStudioWorkspace::load(
                &loaded.paths.root_dir,
                &loaded.paths.data_dir,
                &entry.id,
            )?;
        info!(
            "Loaded workspace '{}' ({}) with species index '{}', action index '{}', and tool catalog '{}'",
            studio_workspace.display_name,
            studio_workspace.units,
            studio_workspace.species_index,
            studio_workspace.actions_index,
            studio_workspace.tool_catalog_path
        );
        info!(
            "Studio loaded {} species, {} action sets, {} tool groups, schema {}",
            studio_workspace.species.len(),
            studio_workspace.action_sets.len(),
            studio_workspace
                .tool_catalog
                .get("tool_groups")
                .and_then(Value::as_array)
                .map_or(0, Vec::len),
            studio_workspace.schema_version
        );
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
    info!(
        "Loaded world '{}' from manifest '{}' (root '{}', data '{}', assets '{}', outputs '{}')",
        loaded.world.name,
        loaded.paths.world_manifest.display(),
        loaded.paths.root_dir.display(),
        loaded.paths.data_dir.display(),
        loaded.paths.assets_dir.display(),
        outputs_dir.display()
    );
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


#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{CharacterStudioWorkspace, read_catalog, validate_relative_path};

    #[test]
    fn checked_in_character_studio_data_cross_references_load() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let data_dir = root.join("worlds/character_studio/data");
        let workspace = CharacterStudioWorkspace::load(root, &data_dir)
            .expect("species, rig, and action references should resolve");

        assert_eq!(workspace.id, "character_studio");
        assert_eq!(workspace.display_name, "Character Studio");
        assert_eq!(workspace.species.len(), 1);
        assert_eq!(workspace.action_sets.len(), 1);
        assert!(
            workspace
                .tool_catalog
                .get("tool_groups")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|groups| groups.len() >= 7)
        );
    }

    #[test]
    fn world_catalog_paths_are_relative_and_contained() {
        assert!(validate_relative_path("worlds/character_studio/world.json").is_ok());
        assert!(validate_relative_path("../outside/world.json").is_err());
        assert!(validate_relative_path("/absolute/world.json").is_err());

        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let catalog = read_catalog(&root.join("worlds/catalog.json"))
            .expect("checked-in world catalog should parse");
        assert!(catalog.worlds.iter().any(|world| world.id == "sandbox"));
        assert!(
            catalog
                .worlds
                .iter()
                .any(|world| world.id == "character_studio")
        );
    }
}
