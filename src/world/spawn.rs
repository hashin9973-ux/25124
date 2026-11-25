use std::collections::HashMap;

use avian3d::prelude::{
    AngularVelocity, CcdFilter, Collider, ColliderDensity, CollisionEventsEnabled, Friction,
    LinearVelocity, Restitution, RigidBody, SweepMode, SweptCcd,
};
use bevy::{gltf::prelude::GltfAssetLabel, prelude::*};

use crate::{
    config::{
        AlphaMode, BodyType, CcdFilterDefinition, ColliderShape, ComponentDefinition,
        RuntimeConfig, SweepModeDefinition, VisualDefinition,
    },
    persistence::{PersistRuntimeState, PersistentObjectId, WorldStateFile},
};

pub(super) fn spawn_world(
    mut commands: Commands,
    config: Res<RuntimeConfig>,
    world_state: Res<WorldStateFile>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut mesh_cache = HashMap::<MeshKey, Handle<Mesh>>::new();
    let mut material_cache = HashMap::<VisualMaterialKey, Handle<StandardMaterial>>::new();

    for instance in &config.world.objects {
        let definition = config
            .components
            .definitions
            .get(&instance.definition)
            .expect("component references are validated before app startup");
        let physical_material = config
            .materials
            .materials
            .get(&definition.material)
            .expect("material references are validated before app startup");

        let saved = if matches!(definition.body_type, BodyType::Dynamic) {
            world_state.bodies.get(&instance.id)
        } else {
            None
        };
        let translation = saved
            .map(|state| Vec3::from_array(state.translation_m))
            .unwrap_or_else(|| Vec3::from_array(instance.position_m));
        let rotation = saved
            .map(|state| Quat::from_array(state.rotation_xyzw))
            .unwrap_or_else(|| rotation_from_degrees(instance.rotation_degrees_xyz));
        let linear_velocity = saved
            .map(|state| Vec3::from_array(state.linear_velocity_m_s))
            .unwrap_or(Vec3::ZERO);
        let angular_velocity = saved
            .map(|state| Vec3::from_array(state.angular_velocity_rad_s))
            .unwrap_or(Vec3::ZERO);

        let rigid_body = match definition.body_type {
            BodyType::Static => RigidBody::Static,
            BodyType::Dynamic => RigidBody::Dynamic,
        };
        let collider = collider_from_definition(&definition.collider);
        let friction = Friction::new(physical_material.dynamic_friction)
            .with_static_coefficient(physical_material.static_friction);

        let mut entity = commands.spawn((
            Name::new(instance.id.clone()),
            PersistentObjectId(instance.id.clone()),
            rigid_body,
            collider,
            ColliderDensity(physical_material.density_kg_m3),
            friction,
            Restitution::new(physical_material.restitution),
            CollisionEventsEnabled,
            LinearVelocity(linear_velocity),
            AngularVelocity(angular_velocity),
            Transform {
                translation,
                rotation,
                ..default()
            },
        ));

        if matches!(definition.body_type, BodyType::Dynamic) {
            entity.insert(PersistRuntimeState);
        }

        if let Some(ccd) = &definition.swept_ccd {
            let filter = match ccd.filter {
                CcdFilterDefinition::Default => CcdFilter::DEFAULT,
                CcdFilterDefinition::Dynamic => CcdFilter::DYNAMIC,
                CcdFilterDefinition::All => CcdFilter::ALL,
                CcdFilterDefinition::None => CcdFilter::NONE,
            };
            let mode = match ccd.mode {
                SweepModeDefinition::Linear => SweepMode::Linear,
                SweepModeDefinition::NonLinear => SweepMode::NonLinear,
            };
            entity.insert(SweptCcd {
                filter,
                mode,
                threshold: ccd.threshold,
            });
        }

        add_visual(
            &mut entity,
            definition,
            &asset_server,
            &mut meshes,
            &mut materials,
            &mut mesh_cache,
            &mut material_cache,
        );
    }

    info!(
        "Loaded world '{}' with {} instances from {} component definitions",
        config.world.name,
        config.world.objects.len(),
        config.components.definitions.len()
    );
}

fn add_visual(
    entity: &mut EntityCommands,
    definition: &ComponentDefinition,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    mesh_cache: &mut HashMap<MeshKey, Handle<Mesh>>,
    material_cache: &mut HashMap<VisualMaterialKey, Handle<StandardMaterial>>,
) {
    if let Some(model_path) = &definition.visual.model {
        // Validation guarantees an optional selector has the form "#SceneN".
        // Unqualified glTF references intentionally select the first scene.
        let (asset_path, scene_index) = match model_path.split_once('#') {
            Some((path, selector)) => {
                let index = selector
                    .strip_prefix("Scene")
                    .and_then(|value| value.parse::<usize>().ok())
                    .expect("model scene selectors are validated before app startup");
                (path, index)
            }
            None => (model_path.as_str(), 0),
        };
        let scene_path = GltfAssetLabel::Scene(scene_index).from_asset(asset_path.to_owned());
        entity.with_children(|children| {
            children.spawn((
                WorldAssetRoot(asset_server.load(scene_path)),
                Transform::default(),
            ));
        });
        return;
    }

    let mesh_key = MeshKey::from_shape(&definition.collider);
    let mesh = mesh_cache
        .entry(mesh_key)
        .or_insert_with(|| mesh_from_collider_shape(&definition.collider, meshes))
        .clone();

    let material_key = VisualMaterialKey::from_visual(&definition.visual);
    let material = material_cache
        .entry(material_key)
        .or_insert_with(|| materials.add(standard_material_from_visual(&definition.visual)))
        .clone();

    entity.insert((Mesh3d(mesh), MeshMaterial3d(material)));
}

fn standard_material_from_visual(visual: &VisualDefinition) -> StandardMaterial {
    StandardMaterial {
        base_color: Color::srgba(
            visual.color_srgba[0],
            visual.color_srgba[1],
            visual.color_srgba[2],
            visual.color_srgba[3],
        ),
        perceptual_roughness: visual.perceptual_roughness,
        metallic: visual.metallic,
        alpha_mode: match visual.alpha_mode {
            AlphaMode::Opaque => bevy::prelude::AlphaMode::Opaque,
            AlphaMode::Blend => bevy::prelude::AlphaMode::Blend,
        },
        ..default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum MeshKey {
    Cuboid([u32; 3]),
    Sphere(u32),
    Capsule(u32, u32),
}

impl MeshKey {
    fn from_shape(shape: &ColliderShape) -> Self {
        match shape {
            ColliderShape::Cuboid { half_extents } => Self::Cuboid([
                half_extents[0].to_bits(),
                half_extents[1].to_bits(),
                half_extents[2].to_bits(),
            ]),
            ColliderShape::Sphere { radius } => Self::Sphere(radius.to_bits()),
            ColliderShape::Capsule {
                radius,
                segment_length,
            } => Self::Capsule(radius.to_bits(), segment_length.to_bits()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct VisualMaterialKey {
    color: [u32; 4],
    roughness: u32,
    metallic: u32,
    alpha_mode: AlphaMode,
}

impl VisualMaterialKey {
    fn from_visual(visual: &VisualDefinition) -> Self {
        Self {
            color: visual.color_srgba.map(f32::to_bits),
            roughness: visual.perceptual_roughness.to_bits(),
            metallic: visual.metallic.to_bits(),
            alpha_mode: visual.alpha_mode,
        }
    }
}

fn collider_from_definition(shape: &ColliderShape) -> Collider {
    match shape {
        ColliderShape::Cuboid { half_extents } => Collider::cuboid(
            half_extents[0] * 2.0,
            half_extents[1] * 2.0,
            half_extents[2] * 2.0,
        ),
        ColliderShape::Sphere { radius } => Collider::sphere(*radius),
        ColliderShape::Capsule {
            radius,
            segment_length,
        } => Collider::capsule(*radius, *segment_length),
    }
}

fn mesh_from_collider_shape(shape: &ColliderShape, meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
    match shape {
        ColliderShape::Cuboid { half_extents } => meshes.add(Cuboid::new(
            half_extents[0] * 2.0,
            half_extents[1] * 2.0,
            half_extents[2] * 2.0,
        )),
        ColliderShape::Sphere { radius } => meshes.add(Sphere::new(*radius).mesh().uv(48, 32)),
        ColliderShape::Capsule {
            radius,
            segment_length,
        } => meshes.add(Capsule3d::new(*radius, *segment_length)),
    }
}

fn rotation_from_degrees(rotation_xyz: [f32; 3]) -> Quat {
    Quat::from_euler(
        EulerRot::XYZ,
        rotation_xyz[0].to_radians(),
        rotation_xyz[1].to_radians(),
        rotation_xyz[2].to_radians(),
    )
}
