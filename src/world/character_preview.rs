use std::collections::HashMap;

use anyhow::{Context, Result, bail, ensure};
use avian3d::prelude::{
    AngularDamping, AngularVelocity, Collider, ColliderDensity, CollisionEventsEnabled,
    Friction, JointCollisionDisabled, JointDamping, LinearDamping, LinearVelocity, Restitution,
    RevoluteJoint, RigidBody, SphericalJoint,
};
use bevy::prelude::*;

use crate::{
    config::{MaterialDefinition, RuntimeConfig},
    editor::{
        CharacterRigDefinition, CharacterRigJoint, CharacterRigSegment, PreviewJointDamping,
    },
    world::WorldInstanceEntity,
};

#[derive(Component, Debug)]
pub struct SpeciesPreviewSegment {
    pub species_id: String,
    pub segment_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum PreviewMeshKey {
    Sphere(u32),
    Capsule(u32, u32),
    Cuboid([u32; 3]),
}

impl PreviewMeshKey {
    fn from_shape(shape: &PreviewShape) -> Self {
        match shape {
            PreviewShape::Sphere { radius } => Self::Sphere(radius.to_bits()),
            PreviewShape::Capsule {
                radius,
                segment_length,
            } => Self::Capsule(radius.to_bits(), segment_length.to_bits()),
            PreviewShape::Cuboid { half_extents } => {
                Self::Cuboid(half_extents.map(f32::to_bits))
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum PreviewShape {
    Sphere { radius: f32 },
    Capsule { radius: f32, segment_length: f32 },
    Cuboid { half_extents: [f32; 3] },
}

impl PreviewShape {
    fn from_segment(segment: &CharacterRigSegment) -> Result<Self> {
        match segment.shape.as_str() {
            "sphere" => {
                let radius = segment.radius_m.context("sphere segment is missing radius_m")?;
                ensure!(radius.is_finite() && radius > 0.0, "sphere radius must be finite and positive");
                Ok(Self::Sphere { radius })
            }
            "capsule" => {
                let radius = segment.radius_m.context("capsule segment is missing radius_m")?;
                let segment_length = segment
                    .length_m
                    .context("capsule segment is missing length_m")?;
                ensure!(
                    radius.is_finite()
                        && radius > 0.0
                        && segment_length.is_finite()
                        && segment_length > 0.0,
                    "capsule dimensions must be finite and positive"
                );
                Ok(Self::Capsule {
                    radius,
                    segment_length,
                })
            }
            "cuboid" => {
                let half_extents = segment
                    .half_extents_m
                    .context("cuboid segment is missing half_extents_m")?;
                ensure!(
                    half_extents.iter().all(|value| value.is_finite() && *value > 0.0),
                    "cuboid half extents must be finite and positive"
                );
                Ok(Self::Cuboid { half_extents })
            }
            other => bail!("unsupported rig segment shape '{other}'"),
        }
    }

    fn collider(self) -> Collider {
        match self {
            Self::Sphere { radius } => Collider::sphere(radius),
            Self::Capsule {
                radius,
                segment_length,
            } => Collider::capsule(radius, segment_length),
            Self::Cuboid { half_extents } => Collider::cuboid(
                half_extents[0] * 2.0,
                half_extents[1] * 2.0,
                half_extents[2] * 2.0,
            ),
        }
    }

    fn mesh(self, meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
        match self {
            Self::Sphere { radius } => meshes.add(Sphere::new(radius).mesh().uv(32, 24)),
            Self::Capsule {
                radius,
                segment_length,
            } => meshes.add(Capsule3d::new(radius, segment_length)),
            Self::Cuboid { half_extents } => meshes.add(Cuboid::new(
                half_extents[0] * 2.0,
                half_extents[1] * 2.0,
                half_extents[2] * 2.0,
            )),
        }
    }
}

pub(super) fn spawn_rig(
    commands: &mut Commands,
    rig: &CharacterRigDefinition,
    config: &RuntimeConfig,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    origin: Vec3,
    joint_damping: &PreviewJointDamping,
) -> Result<()> {
    ensure!(rig.schema_version == 1, "unsupported rig schema for '{}'", rig.species);
    let mut pending = rig.segments.iter().collect::<Vec<_>>();
    let mut body_entities = HashMap::<String, Entity>::with_capacity(rig.segments.len());
    let mut world_transforms = HashMap::<String, Transform>::with_capacity(rig.segments.len());
    let mut mesh_cache = HashMap::<PreviewMeshKey, Handle<Mesh>>::new();

    let preview_visual = config
        .components
        .definitions
        .get("rig.segment_proxy")
        .map(|definition| &definition.visual)
        .context("Character Studio components are missing 'species.segment_proxy'")?;
    let preview_material = materials.add(StandardMaterial {
        base_color: Color::srgba(
            preview_visual.color_srgba[0],
            preview_visual.color_srgba[1],
            preview_visual.color_srgba[2],
            preview_visual.color_srgba[3],
        ),
        perceptual_roughness: preview_visual.perceptual_roughness,
        metallic: preview_visual.metallic,
        alpha_mode: match preview_visual.alpha_mode {
            crate::config::AlphaMode::Opaque => AlphaMode::Opaque,
            crate::config::AlphaMode::Blend => AlphaMode::Blend,
        },
        ..default()
    });

    while !pending.is_empty() {
        let previous_pending = pending.len();
        let mut remaining = Vec::with_capacity(previous_pending);

        for segment in pending {
            let parent_transform = match segment.parent.as_deref() {
                Some(parent_id) => world_transforms.get(parent_id).cloned(),
                None => Some(Transform::from_translation(origin)),
            };
            let Some(parent_transform) = parent_transform else {
                remaining.push(segment);
                continue;
            };

            let shape = PreviewShape::from_segment(segment)
                .with_context(|| format!("invalid segment '{}' in rig '{}'", segment.id, rig.id))?;
            let local_rotation = rotation_from_degrees(segment.local_rotation_degrees_xyz);
            let local_transform = Transform {
                translation: Vec3::from_array(segment.local_center_m),
                rotation: local_rotation,
                ..default()
            };
            let world_transform = parent_transform.mul_transform(local_transform);
            let mesh_key = PreviewMeshKey::from_shape(&shape);
            let mesh = mesh_cache
                .entry(mesh_key)
                .or_insert_with(|| shape.mesh(meshes))
                .clone();

            let physical_material = config
                .materials
                .materials
                .get(&segment.material)
                .with_context(|| {
                    format!(
                        "rig segment '{}' references missing physical material '{}'",
                        segment.id, segment.material
                    )
                })?;
            let entity = spawn_segment(
                commands,
                SegmentSpawnContext {
                    rig,
                    segment,
                    shape,
                    material: physical_material,
                    mesh,
                    visual_material: preview_material.clone(),
                    transform: world_transform,
                },
            )?;

            body_entities.insert(segment.id.clone(), entity);
            world_transforms.insert(segment.id.clone(), world_transform);
        }

        ensure!(
            remaining.len() < previous_pending,
            "rig '{}' could not be resolved into a hierarchy; unresolved parent link or cycle",
            rig.id
        );
        pending = remaining;
    }

    for joint in &rig.joints {
        spawn_joint(commands, rig, joint, &body_entities)?;
    }

    info!(
        "Spawned physical species preview '{}' with {} rigid segments and {} joints",
        rig.species,
        body_entities.len(),
        rig.joints.len()
    );
    Ok(())
}

struct SegmentSpawnContext<'a> {
    rig: &'a CharacterRigDefinition,
    segment: &'a CharacterRigSegment,
    shape: PreviewShape,
    material: &'a MaterialDefinition,
    mesh: Handle<Mesh>,
    visual_material: Handle<StandardMaterial>,
    transform: Transform,
}

fn spawn_segment(
    commands: &mut Commands,
    context: SegmentSpawnContext<'_>,
) -> Result<Entity> {
    let SegmentSpawnContext {
        rig,
        segment,
        shape,
        material,
        mesh,
        visual_material,
        transform,
    } = context;

    ensure!(
        material.density_kg_m3.is_finite() && material.density_kg_m3 > 0.0,
        "segment '{}' has invalid material density",
        segment.id
    );

    let friction = Friction::new(material.dynamic_friction)
        .with_static_coefficient(material.static_friction);
    let entity = commands
        .spawn((
            Name::new(format!("{}::{}", rig.species, segment.id)),
            WorldInstanceEntity,
            SpeciesPreviewSegment {
                species_id: rig.species.clone(),
                segment_id: segment.id.clone(),
            },
            RigidBody::Dynamic,
            shape.collider(),
            ColliderDensity(material.density_kg_m3),
            friction,
            Restitution::new(material.restitution),
            LinearDamping(material.linear_damping),
            AngularDamping(material.angular_damping),
            CollisionEventsEnabled,
            LinearVelocity::ZERO,
            AngularVelocity::ZERO,
            Mesh3d(mesh),
            MeshMaterial3d(visual_material),
            transform,
        ))
        .id();
    Ok(entity)
}

fn spawn_joint(
    commands: &mut Commands,
    rig: &CharacterRigDefinition,
    joint: &CharacterRigJoint,
    bodies: &HashMap<String, Entity>,
) -> Result<()> {
    let body1 = *bodies
        .get(&joint.parent)
        .with_context(|| format!("joint '{}' has unknown parent body '{}'", joint.id, joint.parent))?;
    let body2 = *bodies
        .get(&joint.child)
        .with_context(|| format!("joint '{}' has unknown child body '{}'", joint.id, joint.child))?;

    let anchors = (
        Vec3::from_array(joint.anchor_parent_m),
        Vec3::from_array(joint.anchor_child_m),
    );
    let limits = joint.angular_limits_degrees.as_deref();

    let damping = JointDamping {
        linear: joint_damping.linear,
        angular: joint_damping.angular,
    };

    let name = format!("{}::joint::{}", rig.species, joint.id);
    match joint.joint_type.as_str() {
        "hinge" => {
            let axis = joint
                .axis_local
                .map(Vec3::from_array)
                .context("hinge joint is missing axis_local")?
                .normalize_or_zero();
            ensure!(axis.length_squared() > 0.0, "hinge '{}' has a zero-length axis", joint.id);

            let mut constraint = RevoluteJoint::new(body1, body2)
                .with_hinge_axis(axis)
                .with_local_anchor1(anchors.0)
                .with_local_anchor2(anchors.1);
            if let Some([minimum, maximum]) = limits.and_then(|ranges| ranges.first()).copied() {
                constraint = constraint.with_angle_limits(minimum.to_radians(), maximum.to_radians());
            }
            commands.spawn((
                Name::new(name),
                WorldInstanceEntity,
                JointCollisionDisabled,
                damping,
                constraint,
            ));
        }
        "ball" | "spherical" => {
            let mut constraint = SphericalJoint::new(body1, body2)
                .with_local_anchor1(anchors.0)
                .with_local_anchor2(anchors.1)
                .with_twist_axis(Vec3::Y);
            if let Some(ranges) = limits {
                ensure!(
                    ranges.len() == 3,
                    "spherical joint '{}' must contain three angular ranges",
                    joint.id
                );
                let swing = ranges[0]
                    .into_iter()
                    .chain(ranges[2])
                    .map(f32::abs)
                    .fold(0.0_f32, f32::max)
                    .to_radians();
                let [twist_minimum, twist_maximum] = ranges[1];
                constraint = constraint
                    .with_swing_limits(0.0, swing)
                    .with_twist_limits(twist_minimum.to_radians(), twist_maximum.to_radians());
            }
            commands.spawn((
                Name::new(name),
                WorldInstanceEntity,
                JointCollisionDisabled,
                damping,
                constraint,
            ));
        }
        other => bail!("unsupported joint type '{other}' in rig '{}'", rig.id),
    }

    Ok(())
}

fn rotation_from_degrees(rotation_xyz: [f32; 3]) -> Quat {
    Quat::from_euler(
        EulerRot::XYZ,
        rotation_xyz[0].to_radians(),
        rotation_xyz[1].to_radians(),
        rotation_xyz[2].to_radians(),
    )
}
