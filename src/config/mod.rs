mod load;
mod schema;
mod validate;

pub use load::RuntimeConfig;
pub use schema::{
    BodyType, CameraSettings, ColliderShape, EngineSettings, MaterialCatalog, MaterialDefinition,
    PhysicsSettings, RenderingSettings, VisualDefinition, WorldManifest, WorldObjectDefinition,
};
