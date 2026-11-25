use std::collections::HashSet;
use std::path::{Component, Path};

use anyhow::{Result, ensure};

use super::schema::{ColliderShape, EngineSettings, MaterialCatalog, WorldManifest};

pub fn validate_all(
    engine: &EngineSettings,
    materials: &MaterialCatalog,
    world: &WorldManifest,
) -> Result<()> {
    let physics = &engine.physics;
    ensure!(
        physics.meters_per_unit.is_finite()
            && (physics.meters_per_unit - 1.0).abs() <= f32::EPSILON,
        "physics.meters_per_unit must be exactly 1.0 (one world unit is one metre)"
    );
    ensure!(
        physics.gravity_m_s2.iter().all(|value| value.is_finite()),
        "gravity_m_s2 must contain only finite values"
    );
    ensure!(
        physics.fixed_hz.is_finite() && (30.0..=480.0).contains(&physics.fixed_hz),
        "physics.fixed_hz must be between 30 and 480 Hz"
    );
    ensure!(
        (1..=64).contains(&physics.substeps),
        "physics.substeps must be between 1 and 64"
    );

    let camera = &engine.camera;
    ensure!(
        camera
            .spawn_position_m
            .iter()
            .all(|value| value.is_finite()),
        "camera.spawn_position_m must contain only finite values"
    );
    ensure!(camera.yaw_degrees.is_finite(), "camera yaw must be finite");
    ensure!(
        camera.pitch_degrees.is_finite(),
        "camera pitch must be finite"
    );
    ensure!(
        camera.move_speed_m_s.is_finite() && camera.move_speed_m_s > 0.0,
        "camera move speed must be positive and finite"
    );
    ensure!(
        camera.sprint_multiplier.is_finite() && camera.sprint_multiplier >= 1.0,
        "camera sprint multiplier must be finite and at least 1.0"
    );
    ensure!(
        camera.mouse_radians_per_pixel.is_finite() && camera.mouse_radians_per_pixel >= 0.0,
        "camera mouse sensitivity must be finite and non-negative"
    );

    ensure!(
        engine.rendering.key_light_illuminance_lux.is_finite()
            && engine.rendering.key_light_illuminance_lux > 0.0,
        "key light illuminance must be positive and finite"
    );

    ensure!(
        !materials.materials.is_empty(),
        "material catalogue is empty"
    );
    for (id, material) in &materials.materials {
        ensure!(!id.trim().is_empty(), "material ids cannot be empty");
        ensure!(
            material.density_kg_m3.is_finite() && material.density_kg_m3 > 0.0,
            "material '{id}' density must be positive and finite"
        );
        ensure!(
            material.static_friction.is_finite() && material.static_friction >= 0.0,
            "material '{id}' static friction must be finite and non-negative"
        );
        ensure!(
            material.dynamic_friction.is_finite() && material.dynamic_friction >= 0.0,
            "material '{id}' dynamic friction must be finite and non-negative"
        );
        ensure!(
            material.restitution.is_finite() && (0.0..=1.0).contains(&material.restitution),
            "material '{id}' restitution must be in [0, 1]"
        );
    }

    ensure!(!world.id.trim().is_empty(), "world id cannot be empty");
    ensure!(!world.name.trim().is_empty(), "world name cannot be empty");
    if let Some(gravity) = world.gravity_m_s2 {
        ensure!(
            gravity.iter().all(|value| value.is_finite()),
            "world gravity override must contain only finite values"
        );
    }

    let mut ids = HashSet::with_capacity(world.objects.len());
    for object in &world.objects {
        ensure!(!object.id.trim().is_empty(), "object ids cannot be empty");
        ensure!(
            ids.insert(object.id.as_str()),
            "duplicate world object id '{}'",
            object.id
        );
        ensure!(
            materials.materials.contains_key(&object.material),
            "object '{}' references unknown material '{}'",
            object.id,
            object.material
        );
        ensure!(
            object.position_m.iter().all(|value| value.is_finite()),
            "object '{}' position must contain only finite values",
            object.id
        );
        ensure!(
            object
                .rotation_degrees_xyz
                .iter()
                .all(|value| value.is_finite()),
            "object '{}' rotation must contain only finite values",
            object.id
        );

        match &object.collider {
            ColliderShape::Cuboid { half_extents } => ensure!(
                half_extents
                    .iter()
                    .all(|value| value.is_finite() && *value > 0.0),
                "object '{}' cuboid half_extents must be positive and finite",
                object.id
            ),
            ColliderShape::Sphere { radius } => ensure!(
                radius.is_finite() && *radius > 0.0,
                "object '{}' sphere radius must be positive and finite",
                object.id
            ),
            ColliderShape::Capsule {
                radius,
                segment_length,
            } => ensure!(
                radius.is_finite()
                    && *radius > 0.0
                    && segment_length.is_finite()
                    && *segment_length >= 0.0,
                "object '{}' capsule dimensions are invalid",
                object.id
            ),
        }

        ensure!(
            object
                .visual
                .color_srgba
                .iter()
                .all(|value| { value.is_finite() && (0.0..=1.0).contains(value) }),
            "object '{}' visual color must be finite sRGBA values in [0, 1]",
            object.id
        );
        ensure!(
            object.visual.perceptual_roughness.is_finite()
                && (0.0..=1.0).contains(&object.visual.perceptual_roughness),
            "object '{}' roughness must be in [0, 1]",
            object.id
        );
        ensure!(
            object.visual.metallic.is_finite() && (0.0..=1.0).contains(&object.visual.metallic),
            "object '{}' metallic value must be in [0, 1]",
            object.id
        );

        if let Some(model) = &object.visual.model {
            validate_asset_path(model).map_err(|error| {
                anyhow::anyhow!("object '{}' has invalid model path: {error}", object.id)
            })?;
        }
    }

    Ok(())
}

fn validate_asset_path(asset: &str) -> Result<()> {
    let path_without_scene = asset.split('#').next().unwrap_or_default();
    ensure!(!path_without_scene.trim().is_empty(), "model path is empty");
    ensure!(
        !Path::new(path_without_scene).is_absolute(),
        "model path must be relative to the configured assets directory"
    );

    for component in Path::new(path_without_scene).components() {
        ensure!(
            matches!(component, Component::Normal(_)),
            "model paths cannot contain parent, root, or prefix components"
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::config::RuntimeConfig;

    #[test]
    fn checked_in_data_and_world_are_valid() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let config = RuntimeConfig::load(
            root.to_path_buf(),
            root.join("data"),
            root.join("assets"),
            root.join("worlds/sandbox/world.json"),
        )
        .expect("sample configuration should validate");

        assert_eq!(config.world.id, "sandbox");
        assert_eq!(config.world.objects.len(), 5);
        assert!(config.persisted_state.bodies.is_empty());
    }
}
