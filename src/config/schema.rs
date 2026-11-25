use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineSettings {
    pub schema_version: u32,
    pub physics: PhysicsSettings,
    pub camera: CameraSettings,
    pub rendering: RenderingSettings,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicsSettings {
    pub meters_per_unit: f32,
    pub gravity_m_s2: [f32; 3],
    pub fixed_hz: f64,
    pub substeps: u32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CameraSettings {
    pub spawn_position_m: [f32; 3],
    pub yaw_degrees: f32,
    pub pitch_degrees: f32,
    pub move_speed_m_s: f32,
    pub sprint_multiplier: f32,
    pub mouse_radians_per_pixel: f32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderingSettings {
    pub key_light_illuminance_lux: f32,
    pub key_light_rotation_degrees_xyz: [f32; 3],
    pub shadows_enabled: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialCatalog {
    pub schema_version: u32,
    pub materials: BTreeMap<String, MaterialDefinition>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialDefinition {
    pub density_kg_m3: f32,
    pub static_friction: f32,
    pub dynamic_friction: f32,
    pub restitution: f32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentCatalog {
    pub schema_version: u32,
    pub definitions: BTreeMap<String, ComponentDefinition>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentDefinition {
    pub body_type: BodyType,
    pub collider: ColliderShape,
    pub material: String,
    pub swept_ccd: Option<SweptCcdDefinition>,
    pub visual: VisualDefinition,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CcdFilterDefinition {
    Default,
    Dynamic,
    All,
    None,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SweepModeDefinition {
    Linear,
    NonLinear,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SweptCcdDefinition {
    pub filter: CcdFilterDefinition,
    pub mode: SweepModeDefinition,
    pub threshold: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum BodyType {
    Static,
    Dynamic,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ColliderShape {
    Cuboid { half_extents: [f32; 3] },
    Sphere { radius: f32 },
    Capsule { radius: f32, segment_length: f32 },
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AlphaMode {
    Opaque,
    Blend,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualDefinition {
    pub model: Option<String>,
    pub color_srgba: [f32; 4],
    pub perceptual_roughness: f32,
    pub metallic: f32,
    pub alpha_mode: AlphaMode,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldManifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub gravity_m_s2: Option<[f32; 3]>,
    pub objects: Vec<WorldObjectInstance>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldObjectInstance {
    pub id: String,
    pub definition: String,
    pub position_m: [f32; 3],
    pub rotation_degrees_xyz: [f32; 3],
}
