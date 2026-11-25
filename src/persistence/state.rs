use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, ensure};
use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};

#[derive(Resource, Clone, Debug, Default, Deserialize, Serialize)]
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
        let primary_result = load_valid_state(path, expected_world_id);

        match primary_result {
            Ok(Some(state)) => Ok(state),
            Ok(None) => match load_valid_state(&backup_path, expected_world_id)? {
                Some(state) => {
                    eprintln!(
                        "Warning: recovered world '{}' from backup '{}'",
                        expected_world_id,
                        backup_path.display()
                    );
                    Ok(state)
                }
                None => Ok(Self::new(expected_world_id.to_owned())),
            },
            Err(primary_error) if backup_path.is_file() => {
                match load_valid_state(&backup_path, expected_world_id) {
                    Ok(Some(state)) => {
                        eprintln!(
                            "Warning: primary save '{}' is invalid; recovered world '{}' from backup '{}'. The primary file was preserved.",
                            path.display(),
                            expected_world_id,
                            backup_path.display()
                        );
                        Ok(state)
                    }
                    Ok(None) => Err(primary_error),
                    Err(backup_error) => Err(anyhow!(
                        "primary save '{}' is invalid ({primary_error:#}); backup '{}' is also invalid ({backup_error:#})",
                        path.display(),
                        backup_path.display()
                    )),
                }
            }
            Err(error) => Err(error),
        }
    }

    pub fn write_atomic(&self, path: &Path) -> Result<()> {
        let contents = serde_json::to_vec_pretty(self)?;
        atomic_replace(path, &contents)
    }
}

fn load_valid_state(path: &Path, expected_world_id: &str) -> Result<Option<WorldStateFile>> {
    if !path.is_file() {
        return Ok(None);
    }

    let contents = fs::read_to_string(path)
        .with_context(|| format!("could not read save file '{}'", path.display()))?;
    let mut state: WorldStateFile = serde_json::from_str(&contents)
        .with_context(|| format!("invalid save file '{}'", path.display()))?;

    ensure!(
        state.format_version == 1,
        "unsupported save format version {} in '{}'",
        state.format_version,
        path.display()
    );
    ensure!(
        state.world_id == expected_world_id,
        "save belongs to world '{}', but the active world is '{}'",
        state.world_id,
        expected_world_id
    );

    for (id, body) in &mut state.bodies {
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

        let norm_squared = body
            .rotation_xyzw
            .iter()
            .map(|component| component * component)
            .sum::<f32>();
        ensure!(
            norm_squared.is_finite() && norm_squared > 1.0e-12,
            "save contains a zero-length or invalid quaternion for '{id}'"
        );
        let inverse_norm = norm_squared.sqrt().recip();
        for component in &mut body.rotation_xyzw {
            *component *= inverse_norm;
        }
    }

    Ok(Some(state))
}

fn atomic_replace(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("could not create save directory '{}'", parent.display()))?;

    let temp_path = path.with_extension("json.tmp");
    let backup_path = backup_path(path);

    {
        let mut file = File::create(&temp_path).with_context(|| {
            format!("could not create temporary save '{}'", temp_path.display())
        })?;
        file.write_all(contents)
            .with_context(|| format!("could not write temporary save '{}'", temp_path.display()))?;
        file.sync_all()
            .with_context(|| format!("could not flush temporary save '{}'", temp_path.display()))?;
    }

    let had_previous_save = path.is_file();
    if had_previous_save {
        if backup_path.exists() {
            fs::remove_file(&backup_path).with_context(|| {
                format!(
                    "could not remove old save backup '{}'",
                    backup_path.display()
                )
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

    // A committed primary save is success. A stale backup can safely remain if cleanup fails;
    // the next successful write can replace it.
    if backup_path.exists() {
        let _ = fs::remove_file(&backup_path);
    }

    Ok(())
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

#[cfg(test)]
mod tests {
    use super::{PersistedBodyState, WorldStateFile};

    fn unique_save_path(label: &str) -> PathBuf {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "physics-sandbox-{label}-{}-{timestamp}.json",
            std::process::id()
        ))
    }

    use std::path::PathBuf;

    fn sample_state() -> WorldStateFile {
        let mut state = WorldStateFile::new("test-world".to_owned());
        state.bodies.insert(
            "body-a".to_owned(),
            PersistedBodyState {
                translation_m: [1.0, 2.0, 3.0],
                rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
                linear_velocity_m_s: [4.0, 5.0, 6.0],
                angular_velocity_rad_s: [0.1, 0.2, 0.3],
            },
        );
        state
    }

    #[test]
    fn save_round_trip_preserves_body_state() {
        let path = unique_save_path("roundtrip");
        let expected = sample_state();

        expected.write_atomic(&path).expect("save should succeed");
        let loaded = WorldStateFile::load_or_default(&path, "test-world")
            .expect("save should load");

        assert_eq!(loaded.bodies.len(), 1);
        assert_eq!(loaded.bodies["body-a"].translation_m, [1.0, 2.0, 3.0]);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("json.bak"));
    }

    #[test]
    fn non_unit_quaternion_is_normalized_during_load() {
        let path = unique_save_path("normalize");
        let mut state = sample_state();
        state.bodies.get_mut("body-a").unwrap().rotation_xyzw = [0.0, 0.0, 0.0, 4.0];
        state.write_atomic(&path).expect("save should succeed");

        let loaded = WorldStateFile::load_or_default(&path, "test-world")
            .expect("non-unit quaternion should be normalized");
        assert_eq!(loaded.bodies["body-a"].rotation_xyzw, [0.0, 0.0, 0.0, 1.0]);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn zero_quaternion_is_rejected() {
        let path = unique_save_path("zero-quaternion");
        let mut state = sample_state();
        state.bodies.get_mut("body-a").unwrap().rotation_xyzw = [0.0; 4];
        let contents = serde_json::to_vec(&state).expect("state should serialize");
        std::fs::write(&path, contents).expect("save file should be written");

        assert!(WorldStateFile::load_or_default(&path, "test-world").is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn corrupted_primary_recovers_from_valid_backup() {
        let path = unique_save_path("backup-recovery");
        let backup = path.with_extension("json.bak");
        let valid_state = sample_state();
        std::fs::write(
            &backup,
            serde_json::to_vec(&valid_state).expect("state should serialize"),
        )
        .expect("backup should be written");
        std::fs::write(&path, b"{ invalid json").expect("corrupt primary should be written");

        let loaded = WorldStateFile::load_or_default(&path, "test-world")
            .expect("valid backup should be used");
        assert_eq!(loaded.bodies.len(), 1);
        assert!(std::fs::read_to_string(&path).unwrap().contains("invalid json"));

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&backup);
    }
}
