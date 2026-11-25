use avian3d::prelude::{
    AngularVelocity, Collider, ColliderDensity, CollisionEventsEnabled, Friction, LinearVelocity,
    Restitution, RigidBody, SweptCcd,
};
use bevy::prelude::*;

use crate::{
    config::{BodyType, ColliderShape, RuntimeConfig, WorldObjectDefinition},
    persistence::PersistentObjectId,
};

pub(super) fn spawn_world(
    mut commands: Commands,
    config: Res<RuntimeConfig>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for definition in &config.world.objects {
        let physical_material = config
            .materials
            .materials
            .get(&definition.material)
            .expect("material references are validated before app startup");

        let saved = config.persisted_state.bodies.get(&definition.id);
        let translation = saved
            .map(|state| Vec3::from_array(state.translation_m))
            .unwrap_or_else(|| Vec3::from_array(definition.position_m));
        let rotation = saved
            .map(|state| Quat::from_array(state.rotation_xyzw))
            .unwrap_or_else(|| rotation_from_degrees(definition.rotation_degrees_xyz));
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
            Name::new(definition.id.clone()),
            PersistentObjectId(definition.id.clone()),
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
            entity.insert(SweptCcd::default());
        }

        add_visual(
            &mut entity,
            definition,
            &asset_server,
            &mut meshes,
            &mut materials,
        );
    }

    info!(
        "Loaded world '{}' with {} data-defined objects",
        config.world.name,
        config.world.objects.len()
    );
}

fn add_visual(
    entity: &mut EntityCommands,
    definition: &WorldObjectDefinition,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let visual_material = materials.add(StandardMaterial {
        base_color: Color::srgba(
            definition.visual.color_srgba[0],
            definition.visual.color_srgba[1],
            definition.visual.color_srgba[2],
            definition.visual.color_srgba[3],
        ),
        perceptual_roughness: definition.visual.perceptual_roughness,
        metallic: definition.visual.metallic,
        ..default()
    });

    if let Some(model_path) = &definition.visual.model {
        let scene: Handle<Scene> = asset_server.load(model_path.clone());
        entity.with_children(|children| {
            children.spawn((SceneRoot(scene), Transform::default()));
        });
    } else {
        let mesh = mesh_from_collider_shape(&definition.collider, meshes);
        entity.insert((Mesh3d(mesh), MeshMaterial3d(visual_material)));
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
