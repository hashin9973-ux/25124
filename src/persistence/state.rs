use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorldStateFile {
    pub format_version: u32,
    pub world_id: String,
    pub bodies: BTreeMap<String, PersistedBodyState>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PersistedBodyState {
    pub translation_m: [f32; 3],
    pub rotation_xyzw: [f32; 4],
    pub linear_velocity_m_s: [f32; 3],
    pub angular_velocity_rad_s: [f32; 3],
}

impl WorldStateFile {
    pub fn new(world_id: String) -> Self {
        Self {
            format_version: 1,
            world_id,
            bodies: BTreeMap::new(),
        }
    }

    pub fn load_or_default(path: &Path, expected_world_id: &str) -> Result<Self> {
        let backup_path = backup_path(path);
        let source_path = if path.is_file() {
            Some(path)
        } else if backup_path.is_file() {
            Some(backup_path.as_path())
        } else {
            None
        };

        let Some(source_path) = source_path else {
            return Ok(Self::new(expected_world_id.to_owned()));
        };

        let contents = fs::read_to_string(source_path)
            .with_context(|| format!("could not read save file '{}'", source_path.display()))?;
        let state: Self = serde_json::from_str(&contents)
            .with_context(|| format!("invalid save file '{}'", source_path.display()))?;

        ensure!(
            state.format_version == 1,
            "unsupported save format version {} in '{}'",
            state.format_version,
            source_path.display()
        );
        ensure!(
            state.world_id == expected_world_id,
            "save belongs to world '{}', but the active world is '{}'",
            state.world_id,
            expected_world_id
        );

        for (id, body) in &state.bodies {
            ensure!(!id.trim().is_empty(), "save contains an empty persistent object id");
            ensure!(
                body.translation_m
                    .iter()
                    .chain(body.rotation_xyzw.iter())
                    .chain(body.linear_velocity_m_s.iter())
                    .chain(body.angular_velocity_rad_s.iter())
                    .all(|value| value.is_finite()),
                "save contains non-finite body state for '{id}'"
            );
        }

        Ok(state)
    }

    pub fn write_atomic(&self, path: &Path) -> Result<()> {
        let contents = serde_json::to_vec_pretty(self)?;
        atomic_replace(path, &contents)
    }
}

fn atomic_replace(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("could not create save directory '{}'", parent.display()))?;

    let temp_path = path.with_extension("json.tmp");
    let backup_path = backup_path(path);

    {
        let mut file = File::create(&temp_path)
            .with_context(|| format!("could not create temporary save '{}'", temp_path.display()))?;
        file.write_all(contents)
            .with_context(|| format!("could not write temporary save '{}'", temp_path.display()))?;
        file.sync_all()
            .with_context(|| format!("could not flush temporary save '{}'", temp_path.display()))?;
    }

    let had_previous_save = path.is_file();
    if had_previous_save {
        if backup_path.exists() {
            fs::remove_file(&backup_path).with_context(|| {
                format!("could not remove old save backup '{}'", backup_path.display())
            })?;
        }
        fs::rename(path, &backup_path)
            .with_context(|| format!("could not stage existing save '{}'", path.display()))?;
    }

    if let Err(error) = fs::rename(&temp_path, path) {
        if had_previous_save {
            let _ = fs::rename(&backup_path, path);
        }
        return Err(error)
            .with_context(|| format!("could not commit temporary save '{}'", temp_path.display()));
    }

    if backup_path.exists() {
        fs::remove_file(&backup_path).with_context(|| {
            format!("save committed, but backup '{}' could not be removed", backup_path.display())
        })?;
    }

    Ok(())
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

#[cfg(test)]
mod tests {
    use super::{PersistedBodyState, WorldStateFile};

    #[test]
    fn save_round_trip_preserves_body_state() {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_nanos();
        let unique = format!(
            "physics-sandbox-state-test-{}-{timestamp}.json",
            std::process::id()
        );
        let path = std::env::temp_dir().join(unique);

        let mut expected = WorldStateFile::new("test-world".to_owned());
        expected.bodies.insert(
            "body-a".to_owned(),
            PersistedBodyState {
                translation_m: [1.0, 2.0, 3.0],
                rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
                linear_velocity_m_s: [4.0, 5.0, 6.0],
                angular_velocity_rad_s: [0.1, 0.2, 0.3],
            },
        );

        expected.write_atomic(&path).expect("save should succeed");
        let loaded = WorldStateFile::load_or_default(&path, "test-world")
            .expect("save should load");

        assert_eq!(loaded.bodies.len(), 1);
        assert_eq!(loaded.bodies["body-a"].translation_m, [1.0, 2.0, 3.0]);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("json.bak"));
    }
}
