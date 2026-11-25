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
        atomic_replace(path, &contents, &self.world_id)
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
        ensure!(
            !id.trim().is_empty(),
            "save contains an empty persistent object id"
        );
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReplaceStage {
    TempSynced,
    BackupSynced,
    BackupCommitted,
    PrimaryRemoved,
    PrimaryCommitted,
}

fn atomic_replace(path: &Path, contents: &[u8], expected_world_id: &str) -> Result<()> {
    atomic_replace_with_hook(path, contents, expected_world_id, |_| Ok(()))
}

fn atomic_replace_with_hook<F>(
    path: &Path,
    contents: &[u8],
    expected_world_id: &str,
    mut after_stage: F,
) -> Result<()>
where
    F: FnMut(ReplaceStage) -> Result<()>,
{
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("could not create save directory '{}'", parent.display()))?;

    let temp_path = temp_path(path);
    let backup_path = backup_path(path);
    let backup_temp_path = backup_temp_path(path);

    {
        let mut file = File::create(&temp_path).with_context(|| {
            format!("could not create temporary save '{}'", temp_path.display())
        })?;
        file.write_all(contents)
            .with_context(|| format!("could not write temporary save '{}'", temp_path.display()))?;
        file.sync_all()
            .with_context(|| format!("could not flush temporary save '{}'", temp_path.display()))?;
    }

    // Never replace the current save with bytes that the loader would reject.
    ensure!(
        load_valid_state(&temp_path, expected_world_id)?.is_some(),
        "temporary save '{}' could not be validated",
        temp_path.display()
    );
    sync_directory(parent)?;
    after_stage(ReplaceStage::TempSynced)?;

    // Rotate only a validated primary. If the primary is corrupt but the backup is
    // recoverable, leave that backup untouched throughout the replacement.
    let primary_is_valid = matches!(
        load_valid_state(path, expected_world_id),
        Ok(Some(_))
    );

    if primary_is_valid {
        if backup_temp_path.exists() {
            fs::remove_file(&backup_temp_path).with_context(|| {
                format!(
                    "could not remove stale temporary backup '{}'",
                    backup_temp_path.display()
                )
            })?;
        }

        fs::copy(path, &backup_temp_path).with_context(|| {
            format!(
                "could not stage current save '{}' as '{}'",
                path.display(),
                backup_temp_path.display()
            )
        })?;
        File::open(&backup_temp_path)
            .with_context(|| {
                format!(
                    "could not open staged backup '{}'",
                    backup_temp_path.display()
                )
            })?
            .sync_all()
            .with_context(|| {
                format!(
                    "could not flush staged backup '{}'",
                    backup_temp_path.display()
                )
            })?;
        ensure!(
            load_valid_state(&backup_temp_path, expected_world_id)?.is_some(),
            "staged backup '{}' could not be validated",
            backup_temp_path.display()
        );
        sync_directory(parent)?;
        after_stage(ReplaceStage::BackupSynced)?;

        // The primary is still valid here, so replacing an older backup cannot
        // destroy the last recoverable copy if the process stops mid-rotation.
        if backup_path.exists() {
            fs::remove_file(&backup_path).with_context(|| {
                format!(
                    "could not remove old save backup '{}'",
                    backup_path.display()
                )
            })?;
        }
        fs::rename(&backup_temp_path, &backup_path).with_context(|| {
            format!(
                "could not commit staged backup '{}' as '{}'",
                backup_temp_path.display(),
                backup_path.display()
            )
        })?;
        sync_directory(parent)?;
        after_stage(ReplaceStage::BackupCommitted)?;
    }

    // Rust's rename does not replace an existing destination on every platform.
    // At this point either the old primary is safely backed up, or the pre-existing
    // valid backup (if any) has deliberately been preserved.
    if path.exists() {
        fs::remove_file(path)
            .with_context(|| format!("could not remove previous save '{}'", path.display()))?;
        sync_directory(parent)?;
    }
    after_stage(ReplaceStage::PrimaryRemoved)?;

    fs::rename(&temp_path, path)
        .with_context(|| format!("could not commit temporary save '{}'", temp_path.display()))?;
    sync_directory(parent)?;
    after_stage(ReplaceStage::PrimaryCommitted)?;

    Ok(())
}

fn temp_path(path: &Path) -> PathBuf {
    path.with_extension("json.tmp")
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

fn backup_temp_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak.tmp")
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)
        .with_context(|| format!("could not open save directory '{}'", path.display()))?
        .sync_all()
        .with_context(|| format!("could not sync save directory '{}'", path.display()))
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<()> {
    // Directory handles are not opened portably through std::fs on all platforms.
    // Save contents are still synced before the corresponding rename operations.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        PersistedBodyState, ReplaceStage, WorldStateFile, atomic_replace_with_hook,
        backup_path, backup_temp_path, temp_path,
    };
    use anyhow::anyhow;
    use std::path::{Path, PathBuf};

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

    fn cleanup_save_files(path: &Path) {
        for candidate in [
            path.to_owned(),
            backup_path(path),
            temp_path(path),
            backup_temp_path(path),
        ] {
            let _ = std::fs::remove_file(candidate);
        }
    }

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

    fn interrupt_write_at(
        path: &Path,
        replacement: &WorldStateFile,
        target_stage: ReplaceStage,
    ) {
        let contents =
            serde_json::to_vec_pretty(replacement).expect("replacement state should serialize");
        let mut injected = false;
        let result = atomic_replace_with_hook(path, &contents, &replacement.world_id, |stage| {
            if stage == target_stage {
                injected = true;
                Err(anyhow!("injected interruption at {stage:?}"))
            } else {
                Ok(())
            }
        });

        assert!(injected, "replacement did not reach stage {target_stage:?}");
        assert!(
            result.is_err(),
            "injected interruption at {target_stage:?} should fail the write"
        );
    }

    #[test]
    fn save_round_trip_preserves_body_state() {
        let path = unique_save_path("roundtrip");
        let expected = sample_state();

        expected.write_atomic(&path).expect("save should succeed");
        let loaded =
            WorldStateFile::load_or_default(&path, "test-world").expect("save should load");

        assert_eq!(loaded.bodies.len(), 1);
        assert_eq!(loaded.bodies["body-a"].translation_m, [1.0, 2.0, 3.0]);

        cleanup_save_files(&path);
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

        cleanup_save_files(&path);
    }

    #[test]
    fn zero_quaternion_is_rejected() {
        let path = unique_save_path("zero-quaternion");
        let mut state = sample_state();
        state.bodies.get_mut("body-a").unwrap().rotation_xyzw = [0.0; 4];
        let contents = serde_json::to_vec(&state).expect("state should serialize");
        std::fs::write(&path, contents).expect("save file should be written");

        assert!(WorldStateFile::load_or_default(&path, "test-world").is_err());
        cleanup_save_files(&path);
    }

    #[test]
    fn corrupted_primary_recovers_from_valid_backup() {
        let path = unique_save_path("backup-recovery");
        let backup = backup_path(&path);
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
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("invalid json")
        );

        cleanup_save_files(&path);
    }

    #[test]
    fn interrupted_replacement_with_valid_primary_is_recoverable_at_every_stage() {
        let stages = [
            ReplaceStage::TempSynced,
            ReplaceStage::BackupSynced,
            ReplaceStage::BackupCommitted,
            ReplaceStage::PrimaryRemoved,
            ReplaceStage::PrimaryCommitted,
        ];

        for stage in stages {
            let path = unique_save_path("stage-interruption");
            let mut first_state = sample_state();
            first_state.bodies.get_mut("body-a").unwrap().translation_m = [1.0, 2.0, 3.0];
            first_state.write_atomic(&path).expect("initial save should succeed");

            let mut current_state = sample_state();
            current_state.bodies.get_mut("body-a").unwrap().translation_m = [4.0, 5.0, 6.0];
            current_state
                .write_atomic(&path)
                .expect("second save should create a valid backup");

            let mut replacement = sample_state();
            replacement.bodies.get_mut("body-a").unwrap().translation_m = [7.0, 8.0, 9.0];
            interrupt_write_at(&path, &replacement, stage);

            let recovered = WorldStateFile::load_or_default(&path, "test-world")
                .expect("primary or backup must remain recoverable");
            assert!(
                [
                    [1.0, 2.0, 3.0],
                    [4.0, 5.0, 6.0],
                    [7.0, 8.0, 9.0],
                ]
                .contains(&recovered.bodies["body-a"].translation_m),
                "unexpected recovered state after interruption at {stage:?}"
            );

            cleanup_save_files(&path);
        }
    }

    #[test]
    fn interrupted_save_after_backup_recovery_preserves_known_good_backup() {
        let stages = [
            ReplaceStage::TempSynced,
            ReplaceStage::PrimaryRemoved,
            ReplaceStage::PrimaryCommitted,
        ];

        for stage in stages {
            let path = unique_save_path("recovered-backup-interruption");
            let backup = backup_path(&path);
            let recovered_state = sample_state();
            std::fs::write(
                &backup,
                serde_json::to_vec(&recovered_state).expect("state should serialize"),
            )
            .expect("known-good backup should be written");
            std::fs::write(&path, b"{ corrupt primary").expect("corrupt primary should be written");

            let mut replacement = sample_state();
            replacement.bodies.get_mut("body-a").unwrap().translation_m = [9.0, 8.0, 7.0];
            interrupt_write_at(&path, &replacement, stage);

            let loaded = WorldStateFile::load_or_default(&path, "test-world")
                .expect("interruption must leave a valid primary or backup");
            let expected_translation = if stage == ReplaceStage::PrimaryCommitted {
                [9.0, 8.0, 7.0]
            } else {
                [1.0, 2.0, 3.0]
            };
            assert_eq!(
                loaded.bodies["body-a"].translation_m,
                expected_translation,
                "wrong state recovered after interruption at {stage:?}"
            );

            let preserved_backup = WorldStateFile::load_or_default(&backup, "test-world")
                .expect("known-good backup path should remain valid");
            assert_eq!(
                preserved_backup.bodies["body-a"].translation_m,
                [1.0, 2.0, 3.0]
            );

            cleanup_save_files(&path);
        }
    }
}
