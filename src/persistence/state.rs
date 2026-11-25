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
        let previous_path = previous_path(path);
        let backup_path = backup_path(path);
        let primary_result = load_valid_state(path, expected_world_id);
        let recover = || load_recovery_state([&previous_path, &backup_path], expected_world_id);

        match primary_result {
            Ok(Some(state)) => Ok(state),
            Ok(None) => match recover()? {
                Some((state, recovered_from)) => {
                    eprintln!(
                        "Warning: recovered world '{}' from save recovery file '{}'",
                        expected_world_id,
                        recovered_from.display()
                    );
                    Ok(state)
                }
                None => Ok(Self::new(expected_world_id.to_owned())),
            },
            Err(primary_error) => match recover() {
                Ok(Some((state, recovered_from))) => {
                    eprintln!(
                        "Warning: primary save '{}' is invalid; recovered world '{}' from '{}'. The primary file was preserved.",
                        path.display(),
                        expected_world_id,
                        recovered_from.display()
                    );
                    Ok(state)
                }
                Ok(None) => Err(primary_error),
                Err(recovery_error) => Err(anyhow!(
                    "primary save '{}' is invalid ({primary_error:#}); recovery files are also invalid ({recovery_error:#})",
                    path.display()
                )),
            },
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

fn load_recovery_state(
    candidates: [&Path; 2],
    expected_world_id: &str,
) -> Result<Option<(WorldStateFile, PathBuf)>> {
    let mut errors = Vec::new();

    for candidate in candidates {
        match load_valid_state(candidate, expected_world_id) {
            Ok(Some(state)) => return Ok(Some((state, candidate.to_owned()))),
            Ok(None) => {}
            Err(error) => errors.push(format!("'{}': {error:#}", candidate.display())),
        }
    }

    if errors.is_empty() {
        Ok(None)
    } else {
        Err(anyhow!(
            "no valid recovery save was found ({})",
            errors.join("; ")
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReplaceStage {
    TempSynced,
    PreviousSynced,
    PreviousCommitted,
    PrimaryRemoved,
    PrimaryCommitted,
    BackupRemoved,
    BackupCommitted,
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
    let previous_path = previous_path(path);
    let previous_temp_path = previous_temp_path(path);
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

    // Validate the replacement before touching any existing recovery candidate.
    ensure!(
        load_valid_state(&temp_path, expected_world_id)?.is_some(),
        "temporary save '{}' could not be validated",
        temp_path.display()
    );
    sync_directory(parent)?;
    after_stage(ReplaceStage::TempSynced)?;

    let primary_is_valid = matches!(load_valid_state(path, expected_world_id), Ok(Some(_)));

    if primary_is_valid {
        if previous_temp_path.exists() {
            fs::remove_file(&previous_temp_path).with_context(|| {
                format!(
                    "could not remove stale previous-save temporary '{}'",
                    previous_temp_path.display()
                )
            })?;
        }

        fs::copy(path, &previous_temp_path).with_context(|| {
            format!(
                "could not stage current save '{}' as '{}'",
                path.display(),
                previous_temp_path.display()
            )
        })?;
        File::open(&previous_temp_path)
            .with_context(|| {
                format!(
                    "could not open staged previous save '{}'",
                    previous_temp_path.display()
                )
            })?
            .sync_all()
            .with_context(|| {
                format!(
                    "could not flush staged previous save '{}'",
                    previous_temp_path.display()
                )
            })?;
        ensure!(
            load_valid_state(&previous_temp_path, expected_world_id)?.is_some(),
            "staged previous save '{}' could not be validated",
            previous_temp_path.display()
        );
        sync_directory(parent)?;
        after_stage(ReplaceStage::PreviousSynced)?;

        // The current primary remains in place until the staged copy is durable.
        // The .bak generation is deliberately not modified at this stage.
        if previous_path.exists() {
            fs::remove_file(&previous_path).with_context(|| {
                format!(
                    "could not remove stale previous save '{}'",
                    previous_path.display()
                )
            })?;
        }
        fs::rename(&previous_temp_path, &previous_path).with_context(|| {
            format!(
                "could not commit staged previous save '{}' as '{}'",
                previous_temp_path.display(),
                previous_path.display()
            )
        })?;
        sync_directory(parent)?;
        after_stage(ReplaceStage::PreviousCommitted)?;
    }

    // On Windows, rename does not replace an existing destination. Before removing
    // the primary, either a validated .prev copy exists or an older .bak is preserved.
    if path.exists() {
        fs::remove_file(path).with_context(|| {
            format!(
                "could not remove previous primary save '{}'",
                path.display()
            )
        })?;
        sync_directory(parent)?;
    }
    after_stage(ReplaceStage::PrimaryRemoved)?;

    fs::rename(&temp_path, path)
        .with_context(|| format!("could not commit temporary save '{}'", temp_path.display()))?;
    sync_directory(parent)?;
    after_stage(ReplaceStage::PrimaryCommitted)?;

    // Rotate the staged previous primary into .bak only after the new primary is
    // committed. If .prev came from an earlier interrupted write, the same rule applies.
    if matches!(
        load_valid_state(&previous_path, expected_world_id),
        Ok(Some(_))
    ) {
        if backup_path.exists() {
            fs::remove_file(&backup_path).with_context(|| {
                format!("could not remove old save backup '{}'", backup_path.display())
            })?;
            sync_directory(parent)?;
            after_stage(ReplaceStage::BackupRemoved)?;
        }
        fs::rename(&previous_path, &backup_path).with_context(|| {
            format!(
                "could not rotate previous save '{}' into backup '{}'",
                previous_path.display(),
                backup_path.display()
            )
        })?;
        sync_directory(parent)?;
        after_stage(ReplaceStage::BackupCommitted)?;
    }

    Ok(())
}

fn temp_path(path: &Path) -> PathBuf {
    path.with_extension("json.tmp")
}

fn previous_path(path: &Path) -> PathBuf {
    path.with_extension("json.prev")
}

fn previous_temp_path(path: &Path) -> PathBuf {
    path.with_extension("json.prev.tmp")
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
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
        PersistedBodyState, ReplaceStage, WorldStateFile, atomic_replace_with_hook, backup_path,
        previous_path, previous_temp_path, temp_path,
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
            temp_path(path),
            previous_path(path),
            previous_temp_path(path),
            backup_path(path),
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

    fn interrupt_write_at(path: &Path, replacement: &WorldStateFile, target_stage: ReplaceStage) {
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
            ReplaceStage::PreviousSynced,
            ReplaceStage::PreviousCommitted,
            ReplaceStage::PrimaryRemoved,
            ReplaceStage::PrimaryCommitted,
            ReplaceStage::BackupRemoved,
            ReplaceStage::BackupCommitted,
        ];

        for stage in stages {
            let path = unique_save_path("stage-interruption");
            let mut first_state = sample_state();
            first_state.bodies.get_mut("body-a").unwrap().translation_m = [1.0, 2.0, 3.0];
            first_state
                .write_atomic(&path)
                .expect("initial save should succeed");

            let mut current_state = sample_state();
            current_state
                .bodies
                .get_mut("body-a")
                .unwrap()
                .translation_m = [4.0, 5.0, 6.0];
            current_state
                .write_atomic(&path)
                .expect("second save should create a valid backup");

            let mut replacement = sample_state();
            replacement.bodies.get_mut("body-a").unwrap().translation_m = [7.0, 8.0, 9.0];
            interrupt_write_at(&path, &replacement, stage);

            let recovered = WorldStateFile::load_or_default(&path, "test-world")
                .expect("primary or recovery file must remain recoverable");
            assert!(
                [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0],]
                    .contains(&recovered.bodies["body-a"].translation_m),
                "unexpected recovered state after interruption at {stage:?}"
            );

            let backup_state = WorldStateFile::load_or_default(&backup_path(&path), "test-world");
            if stage == ReplaceStage::BackupRemoved {
                assert!(
                    !backup_path(&path).exists(),
                    "backup should be at the injected post-removal stage"
                );
            } else {
                let backup_state =
                    backup_state.expect("backup should remain valid before rotation");
                let expected_backup_translation = if stage == ReplaceStage::BackupCommitted {
                    [4.0, 5.0, 6.0]
                } else {
                    [1.0, 2.0, 3.0]
                };
                assert_eq!(
                    backup_state.bodies["body-a"].translation_m, expected_backup_translation,
                    "backup was rotated before the primary commit at {stage:?}"
                );
            }

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
                .expect("interruption must leave a valid primary or recovery file");
            let expected_translation = if stage == ReplaceStage::PrimaryCommitted {
                [9.0, 8.0, 7.0]
            } else {
                [1.0, 2.0, 3.0]
            };
            assert_eq!(
                loaded.bodies["body-a"].translation_m, expected_translation,
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

    #[test]
    fn staged_previous_save_is_recovered_before_older_backup() {
        let path = unique_save_path("previous-recovery-order");
        let previous = previous_path(&path);
        let backup = backup_path(&path);
        let mut newer_state = sample_state();
        newer_state.bodies.get_mut("body-a").unwrap().translation_m = [4.0, 5.0, 6.0];
        let older_state = sample_state();

        std::fs::write(
            &previous,
            serde_json::to_vec(&newer_state).expect("previous state should serialize"),
        )
        .expect("previous state should be written");
        std::fs::write(
            &backup,
            serde_json::to_vec(&older_state).expect("backup state should serialize"),
        )
        .expect("backup state should be written");

        let recovered = WorldStateFile::load_or_default(&path, "test-world")
            .expect("staged previous save should be recoverable");
        assert_eq!(recovered.bodies["body-a"].translation_m, [4.0, 5.0, 6.0]);

        cleanup_save_files(&path);
    }
}
