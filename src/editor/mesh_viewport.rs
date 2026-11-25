use std::collections::HashMap;

use anyhow::{Context, Result, ensure};
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

use crate::{
    camera::FreeFlyCamera,
    modeling::{
        MeshDocument, MeshEdge, MeshFace, MeshVertex, save_mesh_document_with_backup,
    },
    world::WorldInstanceEntity,
};

use super::{
    ActiveWorld, CharacterStudioWorkspace, PendingMenuCommand,
};

const VERTEX_PICK_RADIUS_M: f32 = 0.075;
const EDGE_PICK_RADIUS_M: f32 = 0.055;
const PICK_EPSILON: f32 = 1.0e-6;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum MeshElementMode {
    #[default]
    Vertex,
    Edge,
    Face,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SelectedMeshElement {
    Vertex(String),
    Edge(String),
    Face(String),
}

#[derive(Resource, Clone, Debug, Default)]
pub(super) struct MeshViewportState {
    pub(super) mode: MeshElementMode,
    pub(super) active_document_id: Option<String>,
    pub(super) selection: Vec<SelectedMeshElement>,
    pub(super) pointer_click_consumed: bool,
}

#[derive(Component)]
struct MeshDocumentSurface;

#[derive(Resource, Default)]
pub(super) struct RenderedMeshSnapshot {
    document: Option<MeshDocument>,
}

pub(super) fn begin_interaction_frame(mut state: ResMut<MeshViewportState>) {
    state.pointer_click_consumed = false;
}

pub(super) fn sync_mesh_surfaces(
    mut commands: Commands,
    workspace: Res<CharacterStudioWorkspace>,
    state: Res<MeshViewportState>,
    existing_surfaces: Query<Entity, With<MeshDocumentSurface>>,
    mut snapshot: ResMut<RenderedMeshSnapshot>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let selected_document = state
        .active_document_id
        .as_deref()
        .and_then(|id| workspace.mesh_documents.iter().find(|document| document.id == id))
        .or_else(|| workspace.mesh_documents.first());
    let desired_document = selected_document.cloned();

    if snapshot.document == desired_document {
        return;
    }

    for entity in &existing_surfaces {
        commands.entity(entity).despawn();
    }

    if let Some(document) = desired_document.as_ref() {
        match build_surface_mesh(document) {
            Ok(Some(mesh)) => {
                let material = materials.add(StandardMaterial {
                    base_color: Color::srgb(0.58, 0.66, 0.76),
                    perceptual_roughness: 0.82,
                    metallic: 0.0,
                    ..default()
                });
                commands.spawn((
                    Name::new(format!("Mesh surface: {}", document.id)),
                    WorldInstanceEntity,
                    MeshDocumentSurface,
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(material),
                    Transform::default(),
                ));
            }
            Ok(None) => {}
            Err(error) => error!(
                "Could not build rendered surface for mesh '{}': {error:#}",
                document.id
            ),
        }
    }

    snapshot.document = desired_document;
}

fn build_surface_mesh(document: &MeshDocument) -> Result<Option<Mesh>> {
    document.validate()?;
    if document.faces.is_empty() {
        return Ok(None);
    }

    let vertex_positions = document
        .vertices
        .iter()
        .map(|vertex| {
            (
                vertex.id.as_str(),
                vertex.position_m.map(|component| component as f32),
            )
        })
        .collect::<HashMap<_, _>>();

    let mut positions = Vec::<[f32; 3]>::new();
    let mut normals = Vec::<[f32; 3]>::new();
    let mut uvs = Vec::<[f32; 2]>::new();
    let mut indices = Vec::<u32>::new();

    for face in &document.faces {
        let base_index = u32::try_from(positions.len())
            .context("mesh surface contains more vertices than the renderer can index")?;
        let normal = document.face_normal(face)?.map(|component| component as f32);

        for (corner, vertex_id) in face.vertices.iter().enumerate() {
            let position = *vertex_positions
                .get(vertex_id.as_str())
                .with_context(|| format!("face '{}' references missing vertex '{vertex_id}'", face.id))?;
            ensure!(
                position.iter().all(|value| value.is_finite()),
                "mesh '{}' contains a coordinate outside the renderer's float range",
                document.id
            );
            positions.push(position);
            normals.push(normal);
            let uv = face
                .uvs
                .as_ref()
                .map(|coordinates| coordinates[corner].map(|value| value as f32))
                .unwrap_or([0.0, 0.0]);
            ensure!(
                uv.iter().all(|value| value.is_finite()),
                "mesh '{}' contains a UV coordinate outside the renderer's float range",
                document.id
            );
            uvs.push(uv);
        }

        for triangle in document.triangulate_face(face)? {
            for corner_index in triangle {
                indices.push(
                    base_index
                        .checked_add(u32::try_from(corner_index)?)
                        .context("mesh face index overflow")?,
                );
            }
        }
    }

    if indices.is_empty() {
        return Ok(None);
    }

    Ok(Some(
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
            .with_inserted_indices(Indices::U32(indices)),
    ))
}

#[allow(clippy::too_many_arguments)] // Bevy ECS keeps each viewport dependency explicit.
pub(super) fn interact_with_meshes(
    window: Single<&Window, With<bevy::window::PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<FreeFlyCamera>>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    pending: Res<PendingMenuCommand>,
    active_world: Res<ActiveWorld>,
    mut workspace: ResMut<CharacterStudioWorkspace>,
    mut state: ResMut<MeshViewportState>,
) {
    if active_world.kind != Some(super::WorldKind::CharacterStudio) {
        return;
    }

    if keys.just_pressed(KeyCode::Tab) && !workspace.mesh_documents.is_empty() {
        let current_index = state
            .active_document_id
            .as_ref()
            .and_then(|active_id| {
                workspace
                    .mesh_documents
                    .iter()
                    .position(|document| &document.id == active_id)
            })
            .unwrap_or(0);
        let next_index = (current_index + 1) % workspace.mesh_documents.len();
        state.active_document_id =
            Some(workspace.mesh_documents[next_index].id.clone());
        state.selection.clear();
    }

    if keys.just_pressed(KeyCode::Digit1) {
        state.mode = MeshElementMode::Vertex;
        state.selection.clear();
    } else if keys.just_pressed(KeyCode::Digit2) {
        state.mode = MeshElementMode::Edge;
        state.selection.clear();
    } else if keys.just_pressed(KeyCode::Digit3) {
        state.mode = MeshElementMode::Face;
        state.selection.clear();
    }

    if state.active_document_id.as_ref().is_none_or(|id| {
        !workspace.mesh_documents.iter().any(|document| &document.id == id)
    }) {
        state.active_document_id = workspace.mesh_documents.first().map(|document| document.id.clone());
        state.selection.clear();
    }

    if keys.just_pressed(KeyCode::KeyE) && state.mode == MeshElementMode::Vertex {
        if let Err(error) = create_edge_from_selection(&active_world, &mut workspace, &mut state) {
            warn!("Could not create edge from the current vertex selection: {error:#}");
        }
        return;
    }
    if keys.just_pressed(KeyCode::KeyF) && state.mode == MeshElementMode::Vertex {
        if let Err(error) = create_face_from_selection(&active_world, &mut workspace, &mut state) {
            warn!("Could not create face from the current vertex selection: {error:#}");
        }
        return;
    }

    if state.pointer_click_consumed || pending.0.is_some() || !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let (camera, camera_transform) = *camera;
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor) else {
        return;
    };

    let Some(document_id) = state.active_document_id.clone() else {
        return;
    };
    let Some(document_index) = workspace
        .mesh_documents
        .iter()
        .position(|document| document.id == document_id)
    else {
        return;
    };

    let hit = pick_mesh_element(
        &workspace.mesh_documents[document_index],
        ray.origin,
        ray.direction.as_vec3(),
        state.mode,
    );

    if let Some(element) = hit {
        if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
            if let Some(index) = state.selection.iter().position(|existing| existing == &element) {
                state.selection.remove(index);
            } else {
                state.selection.push(element);
            }
        } else {
            state.selection.clear();
            state.selection.push(element);
        }
        return;
    }

    state.selection.clear();
    if state.mode != MeshElementMode::Vertex {
        return;
    }

    let Some(position) = ray_plane_intersection(
        ray.origin,
        ray.direction.as_vec3(),
        workspace.construction_plane_origin_m.map(|value| value as f32),
        workspace.construction_plane_normal_world.map(|value| value as f32),
    ) else {
        return;
    };

    let previous = workspace.mesh_documents[document_index].clone();
    let mut updated = previous.clone();
    let vertex_id = next_vertex_id(&updated);
    let vertex = MeshVertex {
        id: vertex_id.clone(),
        position_m: position.to_array().map(f64::from),
    };
    if let Err(error) = updated.add_vertex(vertex) {
        warn!("Could not add vertex to mesh '{document_id}': {error:#}");
        return;
    }
    if let Err(error) =
        persist_mesh_document(&active_world, &workspace, &previous, &updated)
    {
        error!("Could not persist vertex in mesh '{document_id}': {error:#}");
        return;
    }

    workspace.mesh_documents[document_index] = updated;
    state.selection = vec![SelectedMeshElement::Vertex(vertex_id)];
}

fn pick_mesh_element(
    document: &MeshDocument,
    ray_origin: Vec3,
    ray_direction: Vec3,
    mode: MeshElementMode,
) -> Option<SelectedMeshElement> {
    let mut best_distance = f32::INFINITY;
    let mut best_element = None;

    match mode {
        MeshElementMode::Vertex => {
            for vertex in &document.vertices {
                let position = vertex.position_m.map(|value| value as f32);
                let Some((ray_distance, distance_squared)) =
                    ray_point_distance(ray_origin, ray_direction, Vec3::from_array(position))
                else {
                    continue;
                };
                if distance_squared <= VERTEX_PICK_RADIUS_M * VERTEX_PICK_RADIUS_M
                    && ray_distance < best_distance
                {
                    best_distance = ray_distance;
                    best_element = Some(SelectedMeshElement::Vertex(vertex.id.clone()));
                }
            }
        }
        MeshElementMode::Edge => {
            let positions = document
                .vertices
                .iter()
                .map(|vertex| (vertex.id.as_str(), vertex.position_m))
                .collect::<HashMap<_, _>>();
            for edge in &document.edges {
                let Some(start) = positions.get(edge.vertices[0].as_str()) else {
                    continue;
                };
                let Some(end) = positions.get(edge.vertices[1].as_str()) else {
                    continue;
                };
                let Some((ray_distance, distance_squared)) = ray_segment_distance(
                    ray_origin,
                    ray_direction,
                    Vec3::from_array((*start).map(|value| value as f32)),
                    Vec3::from_array((*end).map(|value| value as f32)),
                ) else {
                    continue;
                };
                if distance_squared <= EDGE_PICK_RADIUS_M * EDGE_PICK_RADIUS_M
                    && ray_distance < best_distance
                {
                    best_distance = ray_distance;
                    best_element = Some(SelectedMeshElement::Edge(edge.id.clone()));
                }
            }
        }
        MeshElementMode::Face => {
            let positions = document
                .vertices
                .iter()
                .map(|vertex| (vertex.id.as_str(), vertex.position_m))
                .collect::<HashMap<_, _>>();
            for face in &document.faces {
                let Ok(triangles) = document.triangulate_face(face) else {
                    continue;
                };
                let Some(face_positions) = face
                    .vertices
                    .iter()
                    .map(|id| positions.get(id.as_str()).copied())
                    .collect::<Option<Vec<_>>>()
                else {
                    continue;
                };

                for triangle in triangles {
                    let a = Vec3::from_array(face_positions[triangle[0]].map(|value| value as f32));
                    let b = Vec3::from_array(face_positions[triangle[1]].map(|value| value as f32));
                    let c = Vec3::from_array(face_positions[triangle[2]].map(|value| value as f32));
                    if let Some(distance) =
                        ray_triangle_distance(ray_origin, ray_direction, a, b, c)
                        && distance < best_distance
                    {
                        best_distance = distance;
                        best_element = Some(SelectedMeshElement::Face(face.id.clone()));
                    }
                }
            }
        }
    }

    best_element
}

fn ray_point_distance(
    origin: Vec3,
    direction: Vec3,
    point: Vec3,
) -> Option<(f32, f32)> {
    let direction = direction.normalize_or_zero();
    if direction == Vec3::ZERO {
        return None;
    }
    let ray_distance = (point - origin).dot(direction);
    if ray_distance < 0.0 {
        return None;
    }
    let closest = origin + direction * ray_distance;
    Some((ray_distance, closest.distance_squared(point)))
}

fn ray_segment_distance(
    origin: Vec3,
    direction: Vec3,
    start: Vec3,
    end: Vec3,
) -> Option<(f32, f32)> {
    let direction = direction.normalize_or_zero();
    let segment = end - start;
    let segment_length_squared = segment.length_squared();
    if direction == Vec3::ZERO || segment_length_squared <= PICK_EPSILON * PICK_EPSILON {
        return None;
    }

    let offset = origin - start;
    let a = direction.length_squared();
    let b = direction.dot(segment);
    let c = segment_length_squared;
    let d = direction.dot(offset);
    let e = segment.dot(offset);
    let denominator = a * c - b * b;

    let (ray_distance, segment_t) = if denominator.abs() <= PICK_EPSILON {
        (0.0_f32, (e / c).clamp(0.0, 1.0))
    } else {
        let mut ray_t = (b * e - c * d) / denominator;
        let mut segment_t = (a * e - b * d) / denominator;
        if ray_t < 0.0 {
            ray_t = 0.0;
            segment_t = (e / c).clamp(0.0, 1.0);
        } else if segment_t < 0.0 {
            segment_t = 0.0;
            ray_t = (-d / a).max(0.0);
        } else if segment_t > 1.0 {
            segment_t = 1.0;
            ray_t = ((b - d) / a).max(0.0);
        }
        (ray_t, segment_t)
    };

    let closest_ray = origin + direction * ray_distance;
    let closest_segment = start + segment * segment_t;
    Some((ray_distance, closest_ray.distance_squared(closest_segment)))
}

fn ray_triangle_distance(
    origin: Vec3,
    direction: Vec3,
    a: Vec3,
    b: Vec3,
    c: Vec3,
) -> Option<f32> {
    let direction = direction.normalize_or_zero();
    if direction == Vec3::ZERO {
        return None;
    }
    let edge_one = b - a;
    let edge_two = c - a;
    let p = direction.cross(edge_two);
    let determinant = edge_one.dot(p);
    if determinant.abs() <= PICK_EPSILON {
        return None;
    }

    let inverse_determinant = determinant.recip();
    let from_a = origin - a;
    let u = from_a.dot(p) * inverse_determinant;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }

    let q = from_a.cross(edge_one);
    let v = direction.dot(q) * inverse_determinant;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }

    let distance = edge_two.dot(q) * inverse_determinant;
    (distance >= 0.0 && distance.is_finite()).then_some(distance)
}

fn ray_plane_intersection(
    ray_origin: Vec3,
    ray_direction: Vec3,
    plane_origin: Vec3,
    plane_normal: Vec3,
) -> Option<Vec3> {
    let direction = ray_direction.normalize_or_zero();
    let normal = plane_normal.normalize_or_zero();
    if direction == Vec3::ZERO || normal == Vec3::ZERO {
        return None;
    }
    let denominator = direction.dot(normal);
    if denominator.abs() <= PICK_EPSILON {
        return None;
    }
    let distance = (plane_origin - ray_origin).dot(normal) / denominator;
    (distance >= 0.0 && distance.is_finite()).then_some(ray_origin + direction * distance)
}

fn create_edge_from_selection(
    active_world: &ActiveWorld,
    workspace: &mut CharacterStudioWorkspace,
    state: &mut MeshViewportState,
) -> Result<()> {
    let document_id = state
        .active_document_id
        .as_deref()
        .context("no active mesh document is selected")?;
    let document_index = workspace
        .mesh_documents
        .iter()
        .position(|document| document.id == document_id)
        .context("active mesh document is not loaded")?;
    let vertex_ids = state
        .selection
        .iter()
        .filter_map(|selection| match selection {
            SelectedMeshElement::Vertex(id) => Some(id.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    ensure!(
        vertex_ids.len() >= 2,
        "select at least two vertices before pressing E"
    );

    let previous = workspace.mesh_documents[document_index].clone();
    let mut updated = previous.clone();
    let start = vertex_ids[vertex_ids.len() - 2].clone();
    let end = vertex_ids[vertex_ids.len() - 1].clone();
    let edge_id = next_edge_id(&updated);
    updated.add_edge(MeshEdge {
        id: edge_id.clone(),
        vertices: [start, end],
    })?;
    persist_mesh_document(active_world, workspace, &previous, &updated)?;
    workspace.mesh_documents[document_index] = updated;
    state.mode = MeshElementMode::Edge;
    state.selection = vec![SelectedMeshElement::Edge(edge_id)];
    Ok(())
}

fn create_face_from_selection(
    active_world: &ActiveWorld,
    workspace: &mut CharacterStudioWorkspace,
    state: &mut MeshViewportState,
) -> Result<()> {
    let document_id = state
        .active_document_id
        .as_deref()
        .context("no active mesh document is selected")?;
    let document_index = workspace
        .mesh_documents
        .iter()
        .position(|document| document.id == document_id)
        .context("active mesh document is not loaded")?;
    let vertex_ids = state
        .selection
        .iter()
        .filter_map(|selection| match selection {
            SelectedMeshElement::Vertex(id) => Some(id.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    ensure!(
        vertex_ids.len() >= 3,
        "select at least three vertices in boundary order before pressing F"
    );

    let previous = workspace.mesh_documents[document_index].clone();
    let mut updated = previous.clone();
    for index in 0..vertex_ids.len() {
        let first = &vertex_ids[index];
        let second = &vertex_ids[(index + 1) % vertex_ids.len()];
        let exists = updated.edges.iter().any(|edge| {
            (edge.vertices[0].as_str() == first.as_str()
                && edge.vertices[1].as_str() == second.as_str())
                || (edge.vertices[0].as_str() == second.as_str()
                    && edge.vertices[1].as_str() == first.as_str())
        });
        if !exists {
            let edge_id = next_edge_id(&updated);
            updated.add_edge(MeshEdge {
                id: edge_id,
                vertices: [first.clone(), second.clone()],
            })?;
        }
    }

    let face_id = next_face_id(&updated);
    updated.add_face(MeshFace {
        id: face_id.clone(),
        vertices: vertex_ids,
        material: None,
        uvs: None,
    })?;
    persist_mesh_document(active_world, workspace, &previous, &updated)?;
    workspace.mesh_documents[document_index] = updated;
    state.mode = MeshElementMode::Face;
    state.selection = vec![SelectedMeshElement::Face(face_id)];
    Ok(())
}

fn persist_mesh_document(
    active_world: &ActiveWorld,
    workspace: &CharacterStudioWorkspace,
    expected_previous: &MeshDocument,
    document: &MeshDocument,
) -> Result<()> {
    let outputs_dir = active_world
        .outputs_dir
        .as_deref()
        .context("active world does not have an outputs directory")?;
    let canonical_outputs = outputs_dir
        .canonicalize()
        .with_context(|| format!("could not resolve world outputs '{}'", outputs_dir.display()))?;
    let candidate = canonical_outputs.join(&workspace.model_output_directory);
    let canonical_directory = candidate.canonicalize().with_context(|| {
        format!("could not resolve mesh output directory '{}'", candidate.display())
    })?;
    ensure!(
        canonical_directory.starts_with(&canonical_outputs),
        "mesh output directory must remain inside the active world outputs"
    );
    let path = canonical_directory.join(format!("{}.mesh.json", document.id));
    save_mesh_document_with_backup(&path, expected_previous, document)
}

fn next_vertex_id(document: &MeshDocument) -> String {
    next_element_id("vertex", document.vertices.iter().map(|item| item.id.as_str()))
}

fn next_edge_id(document: &MeshDocument) -> String {
    next_element_id("edge", document.edges.iter().map(|item| item.id.as_str()))
}

fn next_face_id(document: &MeshDocument) -> String {
    next_element_id("face", document.faces.iter().map(|item| item.id.as_str()))
}

fn next_element_id<'a>(
    prefix: &str,
    existing: impl Iterator<Item = &'a str>,
) -> String {
    let existing = existing.collect::<std::collections::HashSet<_>>();
    let mut sequence = 1_u64;
    loop {
        let candidate = format!("{prefix}_{sequence:03}");
        if !existing.contains(candidate.as_str()) {
            return candidate;
        }
        sequence += 1;
    }
}

fn draw_construction_plane_grid(
    workspace: &CharacterStudioWorkspace,
    gizmos: &mut Gizmos,
) {
    let origin = Vec3::from_array(
        workspace
            .construction_plane_origin_m
            .map(|component| component as f32),
    );
    let normal = Vec3::from_array(
        workspace
            .construction_plane_normal_world
            .map(|component| component as f32),
    )
    .normalize_or_zero();
    if normal == Vec3::ZERO {
        return;
    }

    let reference = if normal.y.abs() < 0.95 {
        Vec3::Y
    } else {
        Vec3::X
    };
    let tangent = normal.cross(reference).normalize_or_zero();
    let bitangent = normal.cross(tangent).normalize_or_zero();
    if tangent == Vec3::ZERO || bitangent == Vec3::ZERO {
        return;
    }

    let spacing = workspace.construction_plane_grid_spacing_m as f32;
    let extent = workspace.construction_plane_grid_extent_m as f32;
    if !spacing.is_finite() || spacing <= 0.0 || !extent.is_finite() || extent <= 0.0 {
        return;
    }

    let intervals = (extent / spacing).floor() as i32;
    let grid_color = Color::srgba(0.32, 0.37, 0.44, 0.55);
    let axis_color = Color::srgba(0.78, 0.82, 0.88, 0.8);
    for index in -intervals..=intervals {
        let offset = index as f32 * spacing;
        let along_tangent = tangent * offset;
        let along_bitangent = bitangent * offset;
        let color = if index == 0 { axis_color } else { grid_color };

        gizmos.line(
            origin + along_tangent - bitangent * extent,
            origin + along_tangent + bitangent * extent,
            color,
        );
        gizmos.line(
            origin + along_bitangent - tangent * extent,
            origin + along_bitangent + tangent * extent,
            color,
        );
    }
}

pub(super) fn draw_mesh_overlay(
    workspace: Res<CharacterStudioWorkspace>,
    state: Res<MeshViewportState>,
    mut gizmos: Gizmos,
) {
    draw_construction_plane_grid(&workspace, &mut gizmos);

    for document in workspace
        .mesh_documents
        .iter()
        .filter(|document| state.active_document_id.as_deref() == Some(document.id.as_str()))
    {
        for edge in &document.edges {
            let Some(start) = document.vertices.iter().find(|vertex| vertex.id == edge.vertices[0])
            else {
                continue;
            };
            let Some(end) = document.vertices.iter().find(|vertex| vertex.id == edge.vertices[1])
            else {
                continue;
            };

            let selected = state.active_document_id.as_deref() == Some(document.id.as_str())
                && state.selection.contains(&SelectedMeshElement::Edge(edge.id.clone()));
            let color = if selected {
                Color::srgb(1.0, 0.72, 0.12)
            } else {
                Color::srgb(0.16, 0.20, 0.25)
            };
            gizmos.line(
                Vec3::from_array(start.position_m.map(|value| value as f32)),
                Vec3::from_array(end.position_m.map(|value| value as f32)),
                color,
            );
        }

        for vertex in &document.vertices {
            let selected = state.active_document_id.as_deref() == Some(document.id.as_str())
                && state
                    .selection
                    .contains(&SelectedMeshElement::Vertex(vertex.id.clone()));
            let color = if selected {
                Color::srgb(1.0, 0.72, 0.12)
            } else {
                Color::srgb(0.88, 0.91, 0.95)
            };
            let center = Vec3::from_array(vertex.position_m.map(|value| value as f32));
            gizmos.sphere(Isometry3d::new(center, Quat::IDENTITY), 0.018, color);
        }

        if state.active_document_id.as_deref() == Some(document.id.as_str()) {
            for selection in &state.selection {
                let SelectedMeshElement::Face(face_id) = selection else {
                    continue;
                };
                let Some(face) = document.faces.iter().find(|face| &face.id == face_id) else {
                    continue;
                };
                let points = face
                    .vertices
                    .iter()
                    .filter_map(|id| {
                        document
                            .vertices
                            .iter()
                            .find(|vertex| &vertex.id == id)
                            .map(|vertex| {
                                Vec3::from_array(vertex.position_m.map(|value| value as f32))
                            })
                    })
                    .collect::<Vec<_>>();
                if points.len() == face.vertices.len() {
                    gizmos.lineloop(points, Color::srgb(1.0, 0.72, 0.12));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::math::Vec3;

    use super::{ray_plane_intersection, ray_triangle_distance};

    #[test]
    fn ray_triangle_pick_reports_nearest_forward_hit_only() {
        let origin = Vec3::new(0.25, 0.25, 2.0);
        let direction = Vec3::NEG_Z;
        let hit = ray_triangle_distance(
            origin,
            direction,
            Vec3::ZERO,
            Vec3::X,
            Vec3::Y,
        )
        .unwrap();
        assert!((hit - 2.0).abs() < 1.0e-6);
        assert!(
            ray_triangle_distance(
                Vec3::new(2.0, 2.0, 2.0),
                direction,
                Vec3::ZERO,
                Vec3::X,
                Vec3::Y,
            )
            .is_none()
        );
    }

    #[test]
    fn construction_plane_intersection_uses_authored_plane_data() {
        let point = ray_plane_intersection(
            Vec3::new(0.0, 2.0, 0.0),
            Vec3::NEG_Y,
            Vec3::new(0.0, 0.5, 0.0),
            Vec3::Y,
        )
        .unwrap();
        assert!((point.y - 0.5).abs() < 1.0e-6);
    }
}
