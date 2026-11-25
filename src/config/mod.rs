mod load;
mod schema;
mod validate;

pub use load::{ProjectPaths, RuntimeConfig};
pub use schema::{
    AlphaMode, BodyType, CameraSettings, ColliderShape, ComponentCatalog, ComponentDefinition,
    EngineSettings, MaterialCatalog, MaterialDefinition, PhysicsSettings, RenderingSettings,
    VisualDefinition, WorldManifest, WorldObjectInstance,
};
