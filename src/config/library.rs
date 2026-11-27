use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use bevy::prelude::Resource;
use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct WorldLibraryEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub schema_version: u32,
    pub region_count: usize,
    pub object_count: usize,
    pub preview_asset: Option<String>,
    pub metadata: BTreeMap<String, serde_json::Value>,
    pub manifest_path: PathBuf,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct WorldLibrary {
    pub entries: Vec<WorldLibraryEntry>,
    pub warnings: Vec<String>,
}

#[derive(Deserialize)]
struct WorldSummary {
    #[serde(default = "default_world_schema_version")]
    schema_version: u32,
    id: String,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    preview_asset: Option<String>,
    #[serde(default)]
    metadata: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    regions: Vec<String>,
    #[serde(default)]
    objects: Vec<serde_json::Value>,
}

fn default_world_schema_version() -> u32 {
    1
}

fn is_safe_preview_asset(value: &str) -> bool {
    let path = Path::new(value);
    !value.trim().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "png" | "jpg" | "jpeg" | "webp"
                )
            })
}

impl WorldLibrary {
    /// Discovers every world.json below the project's worlds directory.
    /// Traversal is deterministic and does not follow directory symlinks.
    /// Unreadable or malformed entries are skipped so one bad world cannot hide the others.
    pub fn discover(project_root: &Path) -> Result<Self> {
        let worlds_dir = project_root.join("worlds");
        if !worlds_dir
            .try_exists()
            .with_context(|| format!("could not inspect '{}'", worlds_dir.display()))?
        {
            return Ok(Self {
                entries: Vec::new(),
                warnings: vec![format!(
                    "world library directory '{}' does not exist; starting with an empty library",
                    worlds_dir.display()
                )],
            });
        }
        ensure!(
            worlds_dir.is_dir(),
            "world library path '{}' exists but is not a directory",
            worlds_dir.display()
        );

        let mut pending = vec![worlds_dir];
        let mut entries = Vec::new();
        let mut warnings = Vec::new();
        while let Some(directory) = pending.pop() {
            let mut children = fs::read_dir(&directory)
                .with_context(|| format!("could not scan '{}'", directory.display()))?
                .collect::<std::io::Result<Vec<_>>>()
                .with_context(|| format!("could not enumerate '{}'", directory.display()))?;
            children.sort_by_key(|entry| entry.file_name());

            for child in children.into_iter().rev() {
                let path = child.path();
                let file_type = child.file_type()?;
                if file_type.is_dir() {
                    pending.push(path);
                    continue;
                }
                if !file_type.is_file() || child.file_name().to_string_lossy() != "world.json" {
                    continue;
                }

                let contents = match fs::read_to_string(&path) {
                    Ok(contents) => contents,
                    Err(error) => {
                        warnings.push(format!("{}: {error}", path.display()));
                        continue;
                    }
                };
                let summary: WorldSummary = match serde_json::from_str(&contents) {
                    Ok(summary) => summary,
                    Err(error) => {
                        warnings.push(format!(
                            "{}: invalid world manifest: {error}",
                            path.display()
                        ));
                        continue;
                    }
                };
                if summary.id.trim().is_empty() || summary.name.trim().is_empty() {
                    warnings.push(format!(
                        "{}: world id and name must both be non-empty",
                        path.display()
                    ));
                    continue;
                }

                let preview_asset = match summary.preview_asset {
                    Some(asset) if is_safe_preview_asset(&asset) => Some(asset),
                    Some(asset) => {
                        warnings.push(format!(
                            "{}: unsafe preview_asset '{}' was ignored; use a relative PNG, JPEG, or WebP path below the assets root",
                            path.display(),
                            asset
                        ));
                        None
                    }
                    None => None,
                };

                entries.push(WorldLibraryEntry {
                    id: summary.id,
                    name: summary.name,
                    description: summary.description,
                    schema_version: summary.schema_version,
                    region_count: summary.regions.len(),
                    object_count: summary.objects.len(),
                    preview_asset,
                    metadata: summary.metadata,
                    manifest_path: path,
                });
            }
        }

        entries.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.id.cmp(&right.id))
                .then_with(|| left.manifest_path.cmp(&right.manifest_path))
        });
        warnings.sort();
        Ok(Self { entries, warnings })
    }
}

#[cfg(test)]
mod tests {
    use super::WorldLibrary;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_root(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("physics-sandbox-library-{label}-{nonce}"))
    }

    #[test]
    fn discovers_all_worlds_recursively_in_stable_order() {
        let root = temp_root("ordered");
        fs::create_dir_all(root.join("worlds/zeta")).unwrap();
        fs::create_dir_all(root.join("worlds/nested/alpha")).unwrap();
        fs::write(
            root.join("worlds/zeta/world.json"),
            r#"{"id":"zeta","name":"Zeta"}"#,
        )
        .unwrap();
        fs::write(
            root.join("worlds/nested/alpha/world.json"),
            r#"{"id":"alpha","name":"Alpha"}"#,
        )
        .unwrap();

        let library = WorldLibrary::discover(&root).unwrap();
        assert_eq!(
            library
                .entries
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            ["alpha", "zeta"]
        );
        assert!(library.warnings.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn world_cards_preserve_manifest_description_metadata_and_preview() {
        let root = temp_root("metadata");
        fs::create_dir_all(root.join("worlds/featured")).unwrap();
        fs::write(
            root.join("worlds/featured/world.json"),
            r#"{
                "schema_version": 2,
                "id": "featured",
                "name": "Featured",
                "description": "A documented world.",
                "preview_asset": "ui/worlds/featured.png",
                "metadata": {"author": "test", "tags": ["physics", "voxel"]},
                "regions": ["regions/a.json", "regions/b.json"],
                "objects": [{"id": "one"}, {"id": "two"}]
            }"#,
        )
        .unwrap();

        let library = WorldLibrary::discover(&root).unwrap();
        let world = &library.entries[0];
        assert_eq!(world.description, "A documented world.");
        assert_eq!(world.schema_version, 2);
        assert_eq!(world.region_count, 2);
        assert_eq!(world.object_count, 2);
        assert_eq!(
            world.preview_asset.as_deref(),
            Some("ui/worlds/featured.png")
        );
        assert_eq!(world.metadata["author"], "test");
        assert!(library.warnings.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unsafe_preview_paths_are_ignored_without_hiding_the_world() {
        let root = temp_root("unsafe-preview");
        fs::create_dir_all(root.join("worlds/world")).unwrap();
        fs::write(
            root.join("worlds/world/world.json"),
            r#"{"id":"world","name":"World","preview_asset":"../outside.png"}"#,
        )
        .unwrap();

        let library = WorldLibrary::discover(&root).unwrap();
        assert_eq!(library.entries.len(), 1);
        assert!(library.entries[0].preview_asset.is_none());
        assert_eq!(library.warnings.len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_world_directory_is_treated_as_an_empty_library() {
        let root = temp_root("missing-worlds");
        fs::create_dir_all(&root).unwrap();

        let library = WorldLibrary::discover(&root).unwrap();
        assert!(library.entries.is_empty());
        assert_eq!(library.warnings.len(), 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn empty_world_library_is_valid() {
        let root = temp_root("empty");
        fs::create_dir_all(root.join("worlds")).unwrap();
        assert!(WorldLibrary::discover(&root).unwrap().entries.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn malformed_world_is_reported_without_hiding_valid_entries() {
        let root = temp_root("malformed");
        fs::create_dir_all(root.join("worlds/valid")).unwrap();
        fs::create_dir_all(root.join("worlds/invalid")).unwrap();
        fs::write(
            root.join("worlds/valid/world.json"),
            r#"{"id":"valid","name":"Valid"}"#,
        )
        .unwrap();
        fs::write(root.join("worlds/invalid/world.json"), "{not valid json").unwrap();

        let library = WorldLibrary::discover(&root).unwrap();
        assert_eq!(library.entries.len(), 1);
        assert_eq!(library.entries[0].id, "valid");
        assert_eq!(library.warnings.len(), 1);
        fs::remove_dir_all(root).unwrap();
    }
}
