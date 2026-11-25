use std::collections::HashSet;

use anyhow::{Result, ensure};

use super::schema::{
    AlphaMode, BodyType, ColliderShape, ComponentCatalog, EngineSettings, MaterialCatalog,
    WorldManifest,
};

const SCHEMA_VERSION: u32 = 1;
const CAMERA_PITCH_LIMIT_DEGREES: f32 = 89.0;

pub fn validate_all(
    engine: &EngineSettings,
    materials: &MaterialCatalog,
    components: &ComponentCatalog,
    world: &WorldManifest,
) -> Result<()> {
    ensure!(
        engine.schema_version == SCHEMA_VERSION,
        "unsupported engine schema version {}",
        engine.schema_version
    );
    ensure!(
        materials.schema_version == SCHEMA_VERSION,
        "unsupported material catalogue schema version {}",
        materials.schema_version
    );
    ensure!(
        components.schema_version == SCHEMA_VERSION,
        "unsupported component catalogue schema version {}",
        components.schema_version
    );
    ensure!(
        world.schema_version == SCHEMA_VERSION,
        "unsupported world schema version {}",
        world.schema_version
    );

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
        camera.pitch_degrees.is_finite()
            && camera.pitch_degrees.abs() <= CAMERA_PITCH_LIMIT_DEGREES,
        "camera pitch must be between -89 and 89 degrees"
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
        engine
            .rendering
            .key_light_rotation_degrees_xyz
            .iter()
            .all(|value| value.is_finite()),
        "key light rotation must contain only finite values"
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

    ensure!(
        !components.definitions.is_empty(),
        "component catalogue is empty"
    );
    for (id, definition) in &components.definitions {
        ensure!(
            !id.trim().is_empty(),
            "component definition ids cannot be empty"
        );
        ensure!(
            materials.materials.contains_key(&definition.material),
            "component definition '{id}' references unknown material '{}'",
            definition.material
        );
        validate_collider(&definition.collider, id)?;
        validate_visual(&definition.visual, id)?;
        if let Some(ccd) = &definition.swept_ccd {
            ensure!(
                matches!(definition.body_type, BodyType::Dynamic),
                "static component definition '{id}' cannot configure swept CCD"
            );
            ensure!(
                ccd.threshold.is_finite() && (0.0..=1.0).contains(&ccd.threshold),
                "component definition '{id}' swept CCD threshold must be finite and in [0, 1]"
            );
        }
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
    for instance in &world.objects {
        ensure!(
            !instance.id.trim().is_empty(),
            "instance ids cannot be empty"
        );
        ensure!(
            ids.insert(instance.id.as_str()),
            "duplicate world instance id '{}'",
            instance.id
        );
        ensure!(
            components.definitions.contains_key(&instance.definition),
            "world instance '{}' references unknown component definition '{}'",
            instance.id,
            instance.definition
        );
        ensure!(
            instance.position_m.iter().all(|value| value.is_finite()),
            "world instance '{}' position must contain only finite values",
            instance.id
        );
        ensure!(
            instance
                .rotation_degrees_xyz
                .iter()
                .all(|value| value.is_finite()),
            "world instance '{}' rotation must contain only finite values",
            instance.id
        );
    }

    Ok(())
}

fn validate_collider(collider: &ColliderShape, owner: &str) -> Result<()> {
    match collider {
        ColliderShape::Cuboid { half_extents } => ensure!(
            half_extents
                .iter()
                .all(|value| value.is_finite() && *value > 0.0),
            "component '{owner}' cuboid half_extents must be positive and finite"
        ),
        ColliderShape::Sphere { radius } => ensure!(
            radius.is_finite() && *radius > 0.0,
            "component '{owner}' sphere radius must be positive and finite"
        ),
        ColliderShape::Capsule {
            radius,
            segment_length,
        } => ensure!(
            radius.is_finite()
                && *radius > 0.0
                && segment_length.is_finite()
                && *segment_length >= 0.0,
            "component '{owner}' capsule dimensions are invalid"
        ),
    }

    Ok(())
}

fn validate_visual(visual: &super::schema::VisualDefinition, owner: &str) -> Result<()> {
    ensure!(
        visual
            .color_srgba
            .iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(value)),
        "component '{owner}' visual color must be finite sRGBA values in [0, 1]"
    );
    ensure!(
        visual.perceptual_roughness.is_finite()
            && (0.0..=1.0).contains(&visual.perceptual_roughness),
        "component '{owner}' roughness must be in [0, 1]"
    );
    ensure!(
        visual.metallic.is_finite() && (0.0..=1.0).contains(&visual.metallic),
        "component '{owner}' metallic value must be in [0, 1]"
    );
    if matches!(visual.alpha_mode, AlphaMode::Opaque) {
        ensure!(
            (visual.color_srgba[3] - 1.0).abs() <= f32::EPSILON,
            "component '{owner}' opaque visual alpha must equal 1.0"
        );
    }

    if let Some(model) = &visual.model {
        validate_asset_path(model).map_err(|error| {
            anyhow::anyhow!("component '{owner}' has invalid model path: {error}")
        })?;
    }

    Ok(())
}

fn validate_asset_path(asset: &str) -> Result<()> {
    let mut parts = asset.split('#');
    let asset_path = parts.next().unwrap_or_default();
    let scene_selector = parts.next();

    ensure!(
        parts.next().is_none(),
        "model reference may contain only one scene selector"
    );
    ensure!(!asset_path.trim().is_empty(), "model path is empty");
    ensure!(
        !asset_path.starts_with('/') && !asset_path.contains('\\'),
        "model paths must be relative and use forward slashes"
    );
    if let Some(selector) = scene_selector {
        let scene_index = selector
            .strip_prefix("Scene")
            .ok_or_else(|| anyhow::anyhow!("model scene selector must use the form '#SceneN'"))?;
        ensure!(
            !scene_index.is_empty() && scene_index.bytes().all(|byte| byte.is_ascii_digit()),
            "model scene selector must use the form '#SceneN'"
        );
        ensure!(
            scene_index.parse::<usize>().is_ok(),
            "model scene selector index is outside the supported range"
        );
    }

    for component in asset_path.split('/') {
        ensure!(
            !component.is_empty() && component != "." && component != "..",
            "model paths cannot contain empty, current-directory, or parent-directory components"
        );
        ensure!(
            !component.contains(':'),
            "model paths cannot contain platform drive or URI prefixes"
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::config::RuntimeConfig;

    #[test]
    fn collider_validation_accepts_valid_shapes_and_rejects_invalid_dimensions() {
        use super::validate_collider;
        use crate::config::ColliderShape;

        assert!(
            validate_collider(
                &ColliderShape::Cuboid {
                    half_extents: [0.5, 0.5, 0.5]
                },
                "valid-cuboid"
            )
            .is_ok()
        );
        assert!(validate_collider(&ColliderShape::Sphere { radius: 0.5 }, "valid-sphere").is_ok());
        assert!(
            validate_collider(
                &ColliderShape::Capsule {
                    radius: 0.25,
                    segment_length: 0.5
                },
                "valid-capsule"
            )
            .is_ok()
        );
        assert!(
            validate_collider(&ColliderShape::Sphere { radius: 0.0 }, "invalid-sphere").is_err()
        );
    }

    #[test]
    fn model_scene_selectors_are_explicit_and_numeric() {
        use super::validate_asset_path;

        assert!(validate_asset_path("models/part.glb").is_ok());
        assert!(validate_asset_path("models/part.glb#Scene0").is_ok());
        assert!(validate_asset_path("models/part.glb#Scene12").is_ok());
        assert!(validate_asset_path("models/part.glb#Scene").is_err());
        assert!(validate_asset_path("models/part.glb#DefaultScene").is_err());
        assert!(validate_asset_path("models/part.glb#Scene-1").is_err());
    }

    #[test]
    fn checked_in_data_and_world_are_valid_without_reading_user_save_data() {
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
        assert_eq!(
            config.world.objects[1].definition,
            config.world.objects[2].definition
        );
    }
}
