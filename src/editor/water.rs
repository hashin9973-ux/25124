use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

use bevy::camera::visibility::NoFrustumCulling;
use bevy::render::render_resource::Face;

use super::menu::MenuCamera;

const GRID: usize = 112;
const WATER_SIZE: f32 = 54.0;
const RIPPLE_LIFETIME: f32 = 5.5;

#[derive(Component)]
pub(super) struct WaterSurface {
    mesh: Handle<Mesh>,
    grid: usize,
    size: f32,
}

#[derive(Clone, Copy, Debug)]
struct Ripple {
    origin: Vec2,
    created_at: f32,
    amplitude: f32,
    frequency: f32,
    speed: f32,
    damping: f32,
}

#[derive(Resource)]
pub(super) struct RippleField {
    ripples: Vec<Ripple>,
    last_cursor: Option<Vec2>,
    last_pointer_impulse: f32,
    random_clock: f32,
    random_state: u64,
}

impl Default for RippleField {
    fn default() -> Self {
        Self {
            ripples: Vec::new(),
            last_cursor: None,
            last_pointer_impulse: f32::NEG_INFINITY,
            random_clock: 0.0,
            random_state: 0x9E37_79B9_7F4A_7C15,
        }
    }
}

impl RippleField {
    fn push(&mut self, origin: Vec2, time: f32, amplitude: f32, frequency: f32, speed: f32) {
        self.ripples.push(Ripple {
            origin,
            created_at: time,
            amplitude,
            frequency,
            speed,
            damping: 1.2,
        });
        if self.ripples.len() > 36 {
            let excess = self.ripples.len() - 36;
            self.ripples.drain(0..excess);
        }
    }

    fn random_unit(&mut self) -> f32 {
        let mut value = self.random_state;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.random_state = value;
        (value as u32) as f32 / u32::MAX as f32
    }
}

pub(super) fn spawn_water(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let mesh = meshes.add(create_water_mesh(GRID, WATER_SIZE));
    let material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.012, 0.075, 0.13),
        perceptual_roughness: 0.089,
        metallic: 0.0,
        reflectance: 0.25,
        ior: 1.333,
        cull_mode: Some(Face::Back),
        ..default()
    });
    commands.spawn((
        Name::new("Procedural water surface"),
        super::menu::MenuScene,
        NoFrustumCulling,
        Mesh3d(mesh.clone()),
        MeshMaterial3d(material),
        Transform::from_xyz(0.0, 0.0, 0.0),
        WaterSurface {
            mesh,
            grid: GRID,
            size: WATER_SIZE,
        },
    ));
}

pub(super) fn animate_water(
    time: Res<Time>,
    mut ripples: ResMut<RippleField>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<bevy::window::PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<MenuCamera>>,
    water: Query<&WaterSurface>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let elapsed = time.elapsed_secs();
    let (camera, camera_transform) = *camera;
    let mut pointer_world = None;

    if let Some(cursor) = window.cursor_position() {
        if let Ok(ray) = camera.viewport_to_world(camera_transform, cursor) {
            pointer_world = ray.plane_intersection_point(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y));
        }

        let moved = ripples
            .last_cursor
            .is_some_and(|last| (cursor - last).length_squared() > 2.0);
        if moved
            && elapsed - ripples.last_pointer_impulse >= 0.075
            && let Some(point) = pointer_world.filter(|point| {
                point.x.abs() <= WATER_SIZE * 0.5 && point.z.abs() <= WATER_SIZE * 0.5
            })
        {
            ripples.push(Vec2::new(point.x, point.z), elapsed, 0.23, 6.4, 4.0);
            ripples.last_pointer_impulse = elapsed;
        }
        if mouse_buttons.just_pressed(MouseButton::Left)
            && let Some(point) = pointer_world.filter(|point| {
                point.x.abs() <= WATER_SIZE * 0.5 && point.z.abs() <= WATER_SIZE * 0.5
            })
        {
            ripples.push(Vec2::new(point.x, point.z), elapsed, 0.62, 5.1, 3.5);
            ripples.last_pointer_impulse = elapsed;
        }
        ripples.last_cursor = Some(cursor);
    } else {
        ripples.last_cursor = None;
    }

    ripples.random_clock += time.delta_secs();
    while ripples.random_clock >= 1.15 {
        ripples.random_clock -= 1.15;
        let x = (ripples.random_unit() - 0.5) * WATER_SIZE * 0.78;
        let z = (ripples.random_unit() - 0.5) * WATER_SIZE * 0.78;
        let amplitude = 0.18 + ripples.random_unit() * 0.18;
        let frequency = 4.8 + ripples.random_unit() * 2.2;
        ripples.push(Vec2::new(x, z), elapsed, amplitude, frequency, 3.2);
    }

    ripples
        .ripples
        .retain(|ripple| elapsed - ripple.created_at <= RIPPLE_LIFETIME);

    for surface in &water {
        let Some(mut mesh) = meshes.get_mut(&surface.mesh) else {
            continue;
        };
        let vertex_count = (surface.grid + 1) * (surface.grid + 1);
        let row = surface.grid + 1;
        let mut positions = Vec::with_capacity(vertex_count);
        let mut normals = Vec::with_capacity(vertex_count);
        let mut uvs = Vec::with_capacity(vertex_count);
        let mut heights = Vec::with_capacity(vertex_count);
        let step = surface.size / surface.grid as f32;
        let half = surface.size * 0.5;

        // Evaluate the wave field once per vertex. Derive normals from the height
        // buffer so active ripples are not recomputed four extra times per vertex.
        for z_index in 0..=surface.grid {
            let z = z_index as f32 * step - half;
            for x_index in 0..=surface.grid {
                let x = x_index as f32 * step - half;
                let height = water_height(Vec2::new(x, z), elapsed, &ripples.ripples);
                heights.push(height);
                positions.push([x, height, z]);
                uvs.push([
                    x_index as f32 / surface.grid as f32 * 7.0,
                    z_index as f32 / surface.grid as f32 * 7.0,
                ]);
            }
        }

        for z_index in 0..=surface.grid {
            for x_index in 0..=surface.grid {
                let left = z_index * row + x_index.saturating_sub(1);
                let right = z_index * row + (x_index + 1).min(surface.grid);
                let back = z_index.saturating_sub(1) * row + x_index;
                let front = (z_index + 1).min(surface.grid) * row + x_index;

                let x_span = if x_index == 0 || x_index == surface.grid {
                    step
                } else {
                    step * 2.0
                };
                let z_span = if z_index == 0 || z_index == surface.grid {
                    step
                } else {
                    step * 2.0
                };
                let dx = (heights[right] - heights[left]) / x_span;
                let dz = (heights[front] - heights[back]) / z_span;
                normals.push(Vec3::new(-dx, 1.0, -dz).normalize().to_array());
            }
        }

        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    }
}

fn water_height(position: Vec2, time: f32, ripples: &[Ripple]) -> f32 {
    let base = 0.065 * (position.x * 0.73 + time * 1.45).sin()
        + 0.045 * (position.y * 0.91 - time * 1.18).sin()
        + 0.035 * ((position.x + position.y) * 0.48 + time * 0.82).sin()
        + 0.022 * ((position.x - position.y) * 0.65 - time * 1.71).sin();

    base + ripples
        .iter()
        .map(|ripple| {
            let age = (time - ripple.created_at).max(0.0);
            let distance = position.distance(ripple.origin);
            let phase = (distance - age * ripple.speed) * ripple.frequency;
            let radial_fade = 1.0 / (1.0 + distance * 0.055);
            ripple.amplitude
                * phase.sin()
                * (-age * ripple.damping).exp()
                * radial_fade
                * (1.0 - (-distance * 0.12).exp())
        })
        .sum::<f32>()
}

fn create_water_mesh(grid: usize, size: f32) -> Mesh {
    let vertex_count = (grid + 1) * (grid + 1);
    let mut positions = Vec::with_capacity(vertex_count);
    let mut normals = Vec::with_capacity(vertex_count);
    let mut uvs = Vec::with_capacity(vertex_count);
    let mut indices = Vec::with_capacity(grid * grid * 6);
    let step = size / grid as f32;
    let half = size * 0.5;

    for z in 0..=grid {
        for x in 0..=grid {
            positions.push([x as f32 * step - half, 0.0, z as f32 * step - half]);
            normals.push([0.0, 1.0, 0.0]);
            uvs.push([x as f32 / grid as f32 * 7.0, z as f32 / grid as f32 * 7.0]);
        }
    }

    let row = grid + 1;
    for z in 0..grid {
        for x in 0..grid {
            let a = (z * row + x) as u32;
            let b = a + 1;
            let c = a + row as u32;
            let d = c + 1;
            indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}
