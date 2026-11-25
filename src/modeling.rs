use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

const DOCUMENT_SCHEMA_VERSION: u32 = 1;
const GEOMETRY_EPSILON: f64 = 1.0e-12;
static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MeshDocument {
    pub schema_version: u32,
    pub id: String,
    pub display_name: String,
    pub units: String,
    #[serde(default)]
    pub vertices: Vec<MeshVertex>,
    #[serde(default)]
    pub edges: Vec<MeshEdge>,
    #[serde(default)]
    pub faces: Vec<MeshFace>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MeshVertex {
    pub id: String,
    pub position_m: [f64; 3],
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MeshEdge {
    pub id: String,
    pub vertices: [String; 2],
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MeshFace {
    pub id: String,
    /// Ordered boundary loop. Every consecutive pair, including the closing pair,
    /// must have an explicit edge in the mesh document.
    pub vertices: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    /// Optional per-corner UV coordinates in the same order as the vertices.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uvs: Option<Vec<[f64; 2]>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtrusionResult {
    pub vertex_ids: Vec<String>,
    pub edge_ids: Vec<String>,
    pub face_ids: Vec<String>,
}

impl MeshDocument {
    pub fn empty(id: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            schema_version: DOCUMENT_SCHEMA_VERSION,
            id: id.into(),
            display_name: display_name.into(),
            units: "meters".to_owned(),
            vertices: Vec::new(),
            edges: Vec::new(),
            faces: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == DOCUMENT_SCHEMA_VERSION,
            "unsupported mesh document schema version {}",
            self.schema_version
        );
        ensure!(!self.id.trim().is_empty(), "mesh document id cannot be empty");
        ensure!(
            !self.display_name.trim().is_empty(),
            "mesh document display name cannot be empty"
        );
        ensure!(!self.units.trim().is_empty(), "mesh document units cannot be empty");

        let mut vertex_ids = HashSet::with_capacity(self.vertices.len());
        for vertex in &self.vertices {
            ensure!(!vertex.id.trim().is_empty(), "mesh contains an empty vertex id");
            ensure!(
                vertex_ids.insert(vertex.id.as_str()),
                "mesh contains duplicate vertex id '{}'",
                vertex.id
            );
            ensure!(
                vertex.position_m
            .iter()
            .all(|value| value.is_finite() && value.abs() <= f32::MAX as f64),
                "vertex '{}' contains a non-finite position",
                vertex.id
            );
        }

        let mut edge_ids = HashSet::with_capacity(self.edges.len());
        let mut endpoint_pairs = HashSet::with_capacity(self.edges.len());
        for edge in &self.edges {
            ensure!(!edge.id.trim().is_empty(), "mesh contains an empty edge id");
            ensure!(
                edge_ids.insert(edge.id.as_str()),
                "mesh contains duplicate edge id '{}'",
                edge.id
            );
            let [first, second] = &edge.vertices;
            ensure!(
                first != second,
                "edge '{}' cannot connect a vertex to itself",
                edge.id
            );
            ensure!(
                vertex_ids.contains(first.as_str()) && vertex_ids.contains(second.as_str()),
                "edge '{}' references an unknown vertex",
                edge.id
            );
            let pair = canonical_edge(first, second);
            ensure!(
                endpoint_pairs.insert(pair),
                "mesh contains duplicate edge between '{}' and '{}'",
                first,
                second
            );

            let first_position = self.vertex_position(first)?;
            let second_position = self.vertex_position(second)?;
            let edge_length_squared = distance_squared(first_position, second_position);
            ensure!(
                edge_length_squared.is_finite()
                    && edge_length_squared > GEOMETRY_EPSILON.powi(2),
                "edge '{}' has zero or numerically invalid geometric length",
                edge.id
            );
        }

        let mut face_ids = HashSet::with_capacity(self.faces.len());
        for face in &self.faces {
            ensure!(!face.id.trim().is_empty(), "mesh contains an empty face id");
            ensure!(
                face_ids.insert(face.id.as_str()),
                "mesh contains duplicate face id '{}'",
                face.id
            );
            ensure!(
                face.vertices.len() >= 3,
                "face '{}' must contain at least three vertices",
                face.id
            );

            let mut loop_ids = HashSet::with_capacity(face.vertices.len());
            for vertex_id in &face.vertices {
                ensure!(
                    vertex_ids.contains(vertex_id.as_str()),
                    "face '{}' references unknown vertex '{}'",
                    face.id,
                    vertex_id
                );
                ensure!(
                    loop_ids.insert(vertex_id.as_str()),
                    "face '{}' repeats vertex '{}'",
                    face.id,
                    vertex_id
                );
            }

            for index in 0..face.vertices.len() {
                let first = &face.vertices[index];
                let second = &face.vertices[(index + 1) % face.vertices.len()];
                ensure!(
                    endpoint_pairs.contains(&canonical_edge(first, second)),
                    "face '{}' is missing boundary edge '{}' -> '{}'",
                    face.id,
                    first,
                    second
                );
            }

            self.face_normal(face)?;
            self.triangulate_face(face)?;
            if let Some(material) = &face.material {
                ensure!(
                    !material.trim().is_empty(),
                    "face '{}' has an empty material reference",
                    face.id
                );
            }
            if let Some(uvs) = &face.uvs {
                ensure!(
                    uvs.len() == face.vertices.len(),
                    "face '{}' UV count must match its vertex count",
                    face.id
                );
                ensure!(
                    uvs.iter().flatten().all(|value| value.is_finite()),
                    "face '{}' contains a non-finite UV coordinate",
                    face.id
                );
            }
        }

        Ok(())
    }

    pub fn add_vertex(&mut self, vertex: MeshVertex) -> Result<()> {
        ensure!(!vertex.id.trim().is_empty(), "vertex id cannot be empty");
        ensure!(
            !self.vertices.iter().any(|existing| existing.id == vertex.id),
            "vertex id '{}' already exists",
            vertex.id
        );
        ensure!(
            vertex.position_m
            .iter()
            .all(|value| value.is_finite() && value.abs() <= f32::MAX as f64),
            "vertex '{}' position must contain only finite values",
            vertex.id
        );
        self.vertices.push(vertex);
        Ok(())
    }

    pub fn add_edge(&mut self, edge: MeshEdge) -> Result<()> {
        ensure!(!edge.id.trim().is_empty(), "edge id cannot be empty");
        ensure!(
            !self.edges.iter().any(|existing| existing.id == edge.id),
            "edge id '{}' already exists",
            edge.id
        );
        let [first, second] = &edge.vertices;
        ensure!(first != second, "edge cannot connect a vertex to itself");
        let first_position = self.vertex_position(first)?;
        let second_position = self.vertex_position(second)?;
        let edge_length_squared = distance_squared(first_position, second_position);
        ensure!(
            edge_length_squared.is_finite()
                && edge_length_squared > GEOMETRY_EPSILON.powi(2),
            "edge '{}' has zero or numerically invalid geometric length",
            edge.id
        );
        ensure!(
            !self.edges.iter().any(|existing| {
                canonical_edge(&existing.vertices[0], &existing.vertices[1])
                    == canonical_edge(first, second)
            }),
            "an edge already connects '{}' and '{}'",
            first,
            second
        );
        self.edges.push(edge);
        Ok(())
    }

    pub fn add_face(&mut self, face: MeshFace) -> Result<()> {
        ensure!(!face.id.trim().is_empty(), "face id cannot be empty");
        ensure!(
            !self.faces.iter().any(|existing| existing.id == face.id),
            "face id '{}' already exists",
            face.id
        );
        self.faces.push(face);
        if let Err(error) = self.validate() {
            self.faces.pop();
            return Err(error);
        }
        Ok(())
    }

    pub fn translate_vertices(
        &mut self,
        vertex_ids: &[String],
        offset_m: [f64; 3],
    ) -> Result<()> {
        ensure!(!vertex_ids.is_empty(), "vertex selection cannot be empty");
        ensure!(
            offset_m.iter().all(|value| value.is_finite()),
            "vertex translation must contain only finite values"
        );

        let selected = vertex_ids.iter().map(String::as_str).collect::<HashSet<_>>();
        ensure!(
            selected.len() == vertex_ids.len(),
            "vertex translation selection contains duplicate ids"
        );
        for id in &selected {
            ensure!(
                self.vertices
                    .iter()
                    .any(|vertex| vertex.id.as_str() == *id),
                "vertex translation references unknown vertex '{}'",
                id
            );
        }

        let mut updated = self.clone();
        for vertex in &mut updated.vertices {
            if selected.contains(vertex.id.as_str()) {
                for (coordinate, offset) in vertex.position_m.iter_mut().zip(offset_m) {
                    *coordinate += offset;
                }
            }
        }
        updated.validate()?;
        *self = updated;
        Ok(())
    }

    /// Extrudes a polygon along its normalized Newell face normal.
    ///
    /// The source face remains as the base. Side quads and a translated cap
    /// are added as new topology. The operation is transactional: on failure,
    /// the original document is unchanged.
    pub fn extrude_face(
        &mut self,
        face_id: &str,
        distance_m: f64,
        id_prefix: &str,
    ) -> Result<ExtrusionResult> {
        ensure!(distance_m.is_finite(), "extrusion distance must be finite");
        ensure!(
            distance_m.abs() > GEOMETRY_EPSILON,
            "extrusion distance must be non-zero"
        );
        ensure!(!id_prefix.trim().is_empty(), "extrusion id prefix cannot be empty");

        let source_face = self
            .faces
            .iter()
            .find(|face| face.id == face_id)
            .cloned()
            .with_context(|| format!("unknown face '{face_id}'"))?;
        let normal = self.face_normal(&source_face)?;
        let mut updated = self.clone();

        let mut result = ExtrusionResult {
            vertex_ids: Vec::with_capacity(source_face.vertices.len()),
            edge_ids: Vec::with_capacity(source_face.vertices.len() * 2),
            face_ids: Vec::with_capacity(source_face.vertices.len() + 1),
        };

        for (index, source_vertex_id) in source_face.vertices.iter().enumerate() {
            let source_position = self.vertex_position(source_vertex_id)?;
            let new_id = format!("{id_prefix}.vertex.{index}");
            let position_m = [
                source_position[0] + normal[0] * distance_m,
                source_position[1] + normal[1] * distance_m,
                source_position[2] + normal[2] * distance_m,
            ];
            updated.add_vertex(MeshVertex {
                id: new_id.clone(),
                position_m,
            })?;
            result.vertex_ids.push(new_id);
        }

        for index in 0..source_face.vertices.len() {
            let next = (index + 1) % source_face.vertices.len();
            let vertical_id = format!("{id_prefix}.edge.vertical.{index}");
            let cap_id = format!("{id_prefix}.edge.cap.{index}");

            updated.add_edge(MeshEdge {
                id: vertical_id.clone(),
                vertices: [
                    source_face.vertices[index].clone(),
                    result.vertex_ids[index].clone(),
                ],
            })?;
            updated.add_edge(MeshEdge {
                id: cap_id.clone(),
                vertices: [
                    result.vertex_ids[index].clone(),
                    result.vertex_ids[next].clone(),
                ],
            })?;
            result.edge_ids.push(vertical_id);
            result.edge_ids.push(cap_id);
        }

        for index in 0..source_face.vertices.len() {
            let next = (index + 1) % source_face.vertices.len();
            let side_id = format!("{id_prefix}.face.side.{index}");
            updated.add_face(MeshFace {
                id: side_id.clone(),
                vertices: vec![
                    source_face.vertices[index].clone(),
                    source_face.vertices[next].clone(),
                    result.vertex_ids[next].clone(),
                    result.vertex_ids[index].clone(),
                ],
                material: source_face.material.clone(),
                uvs: None,
            })?;
            result.face_ids.push(side_id);
        }

        let cap_id = format!("{id_prefix}.face.cap");
        updated.add_face(MeshFace {
            id: cap_id.clone(),
            vertices: result.vertex_ids.clone(),
            material: source_face.material,
            uvs: source_face.uvs,
        })?;
        result.face_ids.push(cap_id);

        updated.validate()?;
        *self = updated;
        Ok(result)
    }

    pub fn face_normal(&self, face: &MeshFace) -> Result<[f64; 3]> {
        ensure!(
            face.vertices.len() >= 3,
            "face '{}' must contain at least three vertices",
            face.id
        );
        let origin = self.vertex_position(&face.vertices[0])?;
        let mut normal = [0.0_f64; 3];
        for index in 0..face.vertices.len() {
            let current = self.vertex_position(&face.vertices[index])?;
            let next = self.vertex_position(&face.vertices[(index + 1) % face.vertices.len()])?;
            let current = subtract_position(current, origin);
            let next = subtract_position(next, origin);

            normal[0] += current[1] * next[2] - current[2] * next[1];
            normal[1] += current[2] * next[0] - current[0] * next[2];
            normal[2] += current[0] * next[1] - current[1] * next[0];
        }

        let length = normal.iter().map(|value| value * value).sum::<f64>().sqrt();
        ensure!(
            length.is_finite() && length > GEOMETRY_EPSILON,
            "face '{}' has degenerate or collinear geometry",
            face.id
        );
        for component in &mut normal {
            *component /= length;
        }
        Ok(normal)
    }

    /// Triangulates a planar, simple polygon using ear clipping.
    ///
    /// Returned indices address the face's ordered corner loop, not the global
    /// vertex array. Concave polygons are supported; self-intersecting and
    /// materially non-planar polygons are rejected instead of being fan-filled.
    pub fn triangulate_face(&self, face: &MeshFace) -> Result<Vec<[usize; 3]>> {
        ensure!(
            face.vertices.len() >= 3,
            "face '{}' must contain at least three vertices",
            face.id
        );

        let positions = face
            .vertices
            .iter()
            .map(|id| self.vertex_position(id))
            .collect::<Result<Vec<_>>>()?;
        let normal = self.face_normal(face)?;
        let origin = positions[0];
        let scale = positions
            .iter()
            .fold([0.0_f64; 3], |mut extent, position| {
                for axis in 0..3 {
                    extent[axis] = extent[axis].max((position[axis] - origin[axis]).abs());
                }
                extent
            })
            .into_iter()
            .fold(0.0_f64, f64::max)
            .max(f64::MIN_POSITIVE);
        let planarity_tolerance = (scale * 1.0e-8).max(1.0e-12);

        for position in &positions {
            let offset = [
                position[0] - origin[0],
                position[1] - origin[1],
                position[2] - origin[2],
            ];
            let distance = offset[0] * normal[0]
                + offset[1] * normal[1]
                + offset[2] * normal[2];
            ensure!(
                distance.abs() <= planarity_tolerance,
                "face '{}' is not planar within tolerance",
                face.id
            );
        }

        let dominant_axis = if normal[0].abs() >= normal[1].abs()
            && normal[0].abs() >= normal[2].abs()
        {
            0
        } else if normal[1].abs() >= normal[2].abs() {
            1
        } else {
            2
        };
        let projected = positions
            .iter()
            .map(|position| {
                let relative = subtract_position(*position, origin);
                match dominant_axis {
                    0 => [relative[1], relative[2]],
                    1 => [relative[0], relative[2]],
                    _ => [relative[0], relative[1]],
                }
            })
            .collect::<Vec<_>>();

        let twice_area = projected
            .iter()
            .enumerate()
            .map(|(index, point)| {
                let next = projected[(index + 1) % projected.len()];
                point[0] * next[1] - next[0] * point[1]
            })
            .sum::<f64>();
        let area_epsilon = (GEOMETRY_EPSILON * scale * scale).max(1.0e-24);
        ensure!(
            twice_area.is_finite() && twice_area.abs() > area_epsilon,
            "face '{}' has zero projected area",
            face.id
        );

        let count = projected.len();
        for first in 0..count {
            let first_next = (first + 1) % count;
            for second in (first + 1)..count {
                let second_next = (second + 1) % count;
                if first == second
                    || first_next == second
                    || second_next == first
                {
                    continue;
                }
                ensure!(
                    !segments_intersect_2d(
                        projected[first],
                        projected[first_next],
                        projected[second],
                        projected[second_next],
                        area_epsilon,
                        (GEOMETRY_EPSILON * scale).max(1.0e-12),
                    ),
                    "face '{}' has a self-intersecting boundary",
                    face.id
                );
            }
        }

        let winding = twice_area.signum();
        let mut remaining = (0..count).collect::<Vec<_>>();
        let mut triangles = Vec::with_capacity(count - 2);

        while remaining.len() > 3 {
            let mut ear = None;
            for cursor in 0..remaining.len() {
                let previous = remaining[(cursor + remaining.len() - 1) % remaining.len()];
                let current = remaining[cursor];
                let next = remaining[(cursor + 1) % remaining.len()];
                let a = projected[previous];
                let b = projected[current];
                let c = projected[next];

                if winding * cross_2d(a, b, c) <= area_epsilon {
                    continue;
                }

                let contains_other_vertex = remaining.iter().copied().any(|candidate| {
                    candidate != previous
                        && candidate != current
                        && candidate != next
                        && point_in_triangle_2d(projected[candidate], a, b, c, winding, area_epsilon)
                });
                if !contains_other_vertex {
                    ear = Some((cursor, [previous, current, next]));
                    break;
                }
            }

            let Some((cursor, triangle)) = ear else {
                anyhow::bail!(
                    "face '{}' could not be triangulated without degeneracy",
                    face.id
                );
            };
            triangles.push(triangle);
            remaining.remove(cursor);
        }

        ensure!(
            winding
                * cross_2d(
                    projected[remaining[0]],
                    projected[remaining[1]],
                    projected[remaining[2]],
                )
                > area_epsilon,
            "face '{}' ends with a degenerate triangle",
            face.id
        );
        triangles.push([remaining[0], remaining[1], remaining[2]]);
        Ok(triangles)
    }

    fn vertex_position(&self, id: &str) -> Result<[f64; 3]> {
        self.vertices
            .iter()
            .find(|vertex| vertex.id == id)
            .map(|vertex| vertex.position_m)
            .with_context(|| format!("unknown vertex '{id}'"))
    }
}

/// Persists an edited mesh document while preserving the previous valid generation.
///
/// The file on disk must still match `expected_previous`; this prevents an
/// editor session from silently overwriting changes made by another process.
pub fn save_mesh_document_with_backup(
    path: &Path,
    expected_previous: &MeshDocument,
    document: &MeshDocument,
) -> Result<()> {
    expected_previous.validate()?;
    document.validate()?;
    ensure!(
        expected_previous.id == document.id,
        "a mesh edit cannot change the document id"
    );

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let parent = parent
        .canonicalize()
        .with_context(|| format!("could not resolve mesh directory '{}'", parent.display()))?;
    ensure!(
        parent.is_dir(),
        "mesh output path '{}' is not a directory",
        parent.display()
    );
    let file_name = path
        .file_name()
        .context("mesh document path must include a file name")?;
    let expected_name = format!("{}.mesh.json", document.id);
    ensure!(
        file_name.to_string_lossy().as_ref() == expected_name.as_str(),
        "mesh document path must be named '{expected_name}'"
    );

    let target = parent.join(file_name);
    let verify_current = || -> Result<()> {
        let metadata = fs::symlink_metadata(&target)
            .with_context(|| format!("mesh document '{}' no longer exists", target.display()))?;
        ensure!(
            metadata.file_type().is_file(),
            "mesh document target '{}' must be a regular file",
            target.display()
        );
        let current_bytes = fs::read(&target)
            .with_context(|| format!("could not read current mesh '{}'", target.display()))?;
        let current: MeshDocument = serde_json::from_slice(&current_bytes)
            .with_context(|| format!("current mesh '{}' is not valid JSON", target.display()))?;
        current.validate()?;
        ensure!(
            current == *expected_previous,
            "mesh '{}' changed on disk since it was loaded; reload before editing",
            document.id
        );
        Ok(())
    };
    verify_current()?;

    let backup = parent.join(format!("{}.prev", file_name.to_string_lossy()));
    let contents = serde_json::to_vec_pretty(document)
        .context("could not serialize mesh document")?;
    let (temporary, mut file) = loop {
        let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(
            ".{}.{}.{}.tmp",
            file_name.to_string_lossy(),
            std::process::id(),
            sequence
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => break (temporary, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("could not create staged mesh file '{}'", temporary.display())
                });
            }
        }
    };

    let write_result = (|| -> Result<()> {
        file.write_all(&contents)
            .with_context(|| format!("could not write staged mesh '{}'", temporary.display()))?;
        file.sync_all()
            .with_context(|| format!("could not flush staged mesh '{}'", temporary.display()))?;
        Ok(())
    })();
    drop(file);

    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }

    let staged_result = (|| -> Result<()> {
        let staged_contents = fs::read(&temporary)
            .with_context(|| format!("could not verify staged mesh '{}'", temporary.display()))?;
        let staged_document: MeshDocument = serde_json::from_slice(&staged_contents)
            .with_context(|| format!("invalid staged mesh '{}'", temporary.display()))?;
        staged_document.validate()?;
        ensure!(
            staged_document == *document,
            "staged mesh '{}' does not match the requested document",
            temporary.display()
        );

        // Recheck immediately before moving the previous generation. This is
        // optimistic concurrency protection, not an OS-level file lock.
        verify_current()?;

        match fs::symlink_metadata(&backup) {
            Ok(metadata) => {
                ensure!(
                    metadata.file_type().is_file(),
                    "mesh recovery file '{}' must be a regular file",
                    backup.display()
                );
                fs::remove_file(&backup).with_context(|| {
                    format!("could not replace previous mesh recovery file '{}'", backup.display())
                })?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("could not inspect previous mesh recovery file"),
        }

        fs::rename(&target, &backup).with_context(|| {
            format!(
                "could not preserve current mesh '{}' as '{}'",
                target.display(),
                backup.display()
            )
        })?;

        if let Err(commit_error) = fs::rename(&temporary, &target) {
            let restore_result = if !target.exists() {
                fs::rename(&backup, &target)
            } else {
                Ok(())
            };
            if let Err(restore_error) = restore_result {
                anyhow::bail!(
                    "could not commit mesh '{}' ({commit_error}); restoring previous file '{}' also failed ({restore_error})",
                    target.display(),
                    backup.display()
                );
            }
            return Err(commit_error).with_context(|| {
                format!("could not commit staged mesh '{}'", target.display())
            });
        }

        #[cfg(unix)]
        let _directory_sync_result =
            fs::File::open(&parent).and_then(|directory| directory.sync_all());

        Ok(())
    })();

    if staged_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    staged_result
}

pub fn discover_mesh_documents(directory: &Path) -> Result<Vec<MeshDocument>> {
    let canonical_directory = directory
        .canonicalize()
        .with_context(|| format!("could not resolve mesh directory '{}'", directory.display()))?;
    ensure!(
        canonical_directory.is_dir(),
        "mesh output path '{}' is not a directory",
        canonical_directory.display()
    );

    let mut paths = fs::read_dir(&canonical_directory)
        .with_context(|| {
            format!(
                "could not read mesh directory '{}'",
                canonical_directory.display()
            )
        })?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;

    // A mesh edit preserves the prior generation as *.mesh.json.prev before
    // replacing the primary file. Restore it if the process stopped in that gap.
    let recovery_paths = paths
        .iter()
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?;
            let primary_name = name.strip_suffix(".prev")?;
            primary_name.ends_with(".mesh.json").then(|| {
                (path.clone(), canonical_directory.join(primary_name))
            })
        })
        .collect::<Vec<_>>();
    for (backup_path, primary_path) in recovery_paths {
        match fs::symlink_metadata(&primary_path) {
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("could not inspect mesh recovery target '{}'", primary_path.display())
                });
            }
        }

        let backup_metadata = fs::symlink_metadata(&backup_path)?;
        ensure!(
            backup_metadata.file_type().is_file(),
            "mesh recovery file '{}' must be a regular file",
            backup_path.display()
        );
        let canonical_backup = backup_path
            .canonicalize()
            .with_context(|| format!("could not resolve mesh recovery file '{}'", backup_path.display()))?;
        ensure!(
            canonical_backup.starts_with(&canonical_directory),
            "mesh recovery file '{}' resolves outside its world-owned directory",
            backup_path.display()
        );
        let contents = fs::read(&canonical_backup)?;
        let recovered: MeshDocument = serde_json::from_slice(&contents)
            .with_context(|| format!("invalid mesh recovery file '{}'", backup_path.display()))?;
        recovered.validate().with_context(|| {
            format!("mesh recovery file '{}' failed validation", backup_path.display())
        })?;
        let expected_name = format!("{}.mesh.json", recovered.id);
        ensure!(
            primary_path.file_name().is_some_and(|name| {
                name.to_string_lossy().as_ref() == expected_name.as_str()
            }),
            "mesh recovery file '{}' filename does not match id '{}'",
            backup_path.display(),
            recovered.id
        );
        fs::rename(&canonical_backup, &primary_path).with_context(|| {
            format!(
                "could not restore mesh recovery file '{}' to '{}'",
                backup_path.display(),
                primary_path.display()
            )
        })?;
        paths.push(primary_path);
    }
    paths.retain(|path| path.exists());
    paths.sort();

    let mut documents = Vec::new();
    let mut ids = HashSet::new();
    for path in paths {
        let file_type = fs::symlink_metadata(&path)?.file_type();
        if !file_type.is_file()
            || !path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().ends_with(".mesh.json"))
        {
            continue;
        }

        let canonical_path = path
            .canonicalize()
            .with_context(|| format!("could not resolve mesh document '{}'", path.display()))?;
        ensure!(
            canonical_path.starts_with(&canonical_directory),
            "mesh document '{}' resolves outside its world-owned directory",
            path.display()
        );
        let contents = fs::read_to_string(&canonical_path).with_context(|| {
            format!(
                "could not read mesh document '{}'",
                canonical_path.display()
            )
        })?;
        let document: MeshDocument = serde_json::from_str(&contents)
            .with_context(|| format!("invalid mesh document '{}'", canonical_path.display()))?;
        document.validate().with_context(|| {
            format!("mesh document '{}' failed validation", canonical_path.display())
        })?;

        let expected_name = format!("{}.mesh.json", document.id);
        ensure!(
            path.file_name().is_some_and(|name| {
                name.to_string_lossy().as_ref() == expected_name.as_str()
            }),
            "mesh document '{}' filename does not match id '{}'",
            path.display(),
            document.id
        );
        ensure!(
            ids.insert(document.id.clone()),
            "mesh directory contains duplicate document id '{}'",
            document.id
        );
        documents.push(document);
    }

    Ok(documents)
}

/// Creates a new empty mesh document without overwriting existing user content.
pub fn create_empty_mesh_document(output_directory: &Path) -> Result<MeshDocument> {
    fs::create_dir_all(output_directory).with_context(|| {
        format!(
            "could not create mesh output directory '{}'",
            output_directory.display()
        )
    })?;
    let canonical_directory = output_directory
        .canonicalize()
        .with_context(|| {
            format!(
                "could not resolve mesh output directory '{}'",
                output_directory.display()
            )
        })?;
    let mut sequence = 1_u32;

    loop {
        let id = format!("mesh_{sequence:03}");
        let display_name = format!("Untitled Mesh {sequence}");
        let path = canonical_directory.join(format!("{id}.mesh.json"));
        let document = MeshDocument::empty(id, display_name);
        document.validate()?;
        let contents = serde_json::to_vec_pretty(&document)
            .context("could not serialize new mesh document")?;

        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                sequence = sequence
                    .checked_add(1)
                    .context("mesh identifier sequence is exhausted")?;
                continue;
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("could not create mesh document '{}'", path.display())
                });
            }
        };

        if let Err(error) = file
            .write_all(&contents)
            .and_then(|()| file.sync_all())
        {
            drop(file);
            let _ = fs::remove_file(&path);
            return Err(error).with_context(|| {
                format!("could not persist new mesh document '{}'", path.display())
            });
        }

        return Ok(document);
    }
}

fn cross_2d(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn subtract_position(first: [f64; 3], second: [f64; 3]) -> [f64; 3] {
    [
        first[0] - second[0],
        first[1] - second[1],
        first[2] - second[2],
    ]
}

fn point_in_triangle_2d(
    point: [f64; 2],
    a: [f64; 2],
    b: [f64; 2],
    c: [f64; 2],
    winding: f64,
    epsilon: f64,
) -> bool {
    winding * cross_2d(a, b, point) >= -epsilon
        && winding * cross_2d(b, c, point) >= -epsilon
        && winding * cross_2d(c, a, point) >= -epsilon
}

fn segments_intersect_2d(
    a: [f64; 2],
    b: [f64; 2],
    c: [f64; 2],
    d: [f64; 2],
    area_epsilon: f64,
    linear_epsilon: f64,
) -> bool {
    let ab_c = cross_2d(a, b, c);
    let ab_d = cross_2d(a, b, d);
    let cd_a = cross_2d(c, d, a);
    let cd_b = cross_2d(c, d, b);

    if ab_c * ab_d < -area_epsilon * area_epsilon
        && cd_a * cd_b < -area_epsilon * area_epsilon
    {
        return true;
    }

    let on_segment = |point: [f64; 2], start: [f64; 2], end: [f64; 2], cross: f64| {
        cross.abs() <= area_epsilon
            && point[0] >= start[0].min(end[0]) - linear_epsilon
            && point[0] <= start[0].max(end[0]) + linear_epsilon
            && point[1] >= start[1].min(end[1]) - linear_epsilon
            && point[1] <= start[1].max(end[1]) + linear_epsilon
    };

    on_segment(c, a, b, ab_c)
        || on_segment(d, a, b, ab_d)
        || on_segment(a, c, d, cd_a)
        || on_segment(b, c, d, cd_b)
}

fn canonical_edge(first: &str, second: &str) -> (String, String) {
    if first <= second {
        (first.to_owned(), second.to_owned())
    } else {
        (second.to_owned(), first.to_owned())
    }
}

fn distance_squared(first: [f64; 3], second: [f64; 3]) -> f64 {
    first
        .into_iter()
        .zip(second)
        .map(|(first, second)| {
            let difference = first - second;
            difference * difference
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::{
        MeshDocument, MeshEdge, MeshFace, MeshVertex, create_empty_mesh_document,
        discover_mesh_documents,
    };

    fn empty_document() -> MeshDocument {
        MeshDocument::empty("mesh_test", "Test Mesh")
    }

    fn triangle_document() -> MeshDocument {
        let mut mesh = empty_document();
        for (id, position_m) in [
            ("v0", [0.0, 0.0, 0.0]),
            ("v1", [1.0, 0.0, 0.0]),
            ("v2", [0.0, 1.0, 0.0]),
        ] {
            mesh.add_vertex(MeshVertex {
                id: id.to_owned(),
                position_m,
            })
            .unwrap();
        }
        for (id, vertices) in [
            ("e0", ["v0", "v1"]),
            ("e1", ["v1", "v2"]),
            ("e2", ["v2", "v0"]),
        ] {
            mesh.add_edge(MeshEdge {
                id: id.to_owned(),
                vertices: vertices.map(str::to_owned),
            })
            .unwrap();
        }
        mesh.add_face(MeshFace {
            id: "f0".to_owned(),
            vertices: vec!["v0".to_owned(), "v1".to_owned(), "v2".to_owned()],
            material: Some("surface_default".to_owned()),
            uvs: Some(vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]),
        })
        .unwrap();
        mesh
    }

    fn temporary_directory(label: &str) -> PathBuf {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "physics-sandbox-{label}-{}-{timestamp}",
            std::process::id()
        ))
    }

    #[test]
    fn empty_mesh_is_valid_and_contains_no_authored_geometry() {
        let mesh = empty_document();
        mesh.validate().unwrap();
        assert!(mesh.vertices.is_empty());
        assert!(mesh.edges.is_empty());
        assert!(mesh.faces.is_empty());
    }

    #[test]
    fn face_requires_explicit_boundary_edges_and_valid_uvs() {
        let mut mesh = triangle_document();
        mesh.edges.pop();
        assert!(mesh.validate().is_err());

        let mut mesh = triangle_document();
        mesh.faces[0].uvs = Some(vec![[0.0, 0.0], [1.0, 0.0]]);
        assert!(mesh.validate().is_err());
    }

    #[test]
    fn vertex_translation_is_atomic_and_validates_references() {
        let mut mesh = triangle_document();
        let original = mesh.clone();
        assert!(
            mesh.translate_vertices(&["missing".to_owned()], [1.0, 0.0, 0.0])
                .is_err()
        );
        assert_eq!(mesh, original);

        mesh.translate_vertices(&["v0".to_owned()], [0.0, 0.0, 2.0])
            .unwrap();
        assert_eq!(mesh.vertices[0].position_m, [0.0, 0.0, 2.0]);
        mesh.validate().unwrap();
    }

    #[test]
    fn face_extrusion_creates_connected_cap_and_side_topology() {
        let mut mesh = triangle_document();
        let result = mesh.extrude_face("f0", 2.0, "op001").unwrap();

        assert_eq!(result.vertex_ids.len(), 3);
        assert_eq!(result.edge_ids.len(), 6);
        assert_eq!(result.face_ids.len(), 4);
        assert_eq!(mesh.vertices.len(), 6);
        assert_eq!(mesh.edges.len(), 9);
        assert_eq!(mesh.faces.len(), 5);
        for id in &result.vertex_ids {
            let vertex = mesh.vertices.iter().find(|vertex| &vertex.id == id).unwrap();
            assert!((vertex.position_m[2] - 2.0).abs() < 1.0e-9);
        }
        mesh.validate().unwrap();
    }

    #[test]
    fn failed_extrusion_does_not_mutate_the_original_document() {
        let mut mesh = triangle_document();
        mesh.extrude_face("f0", 1.0, "existing").unwrap();
        let after_first = mesh.clone();
        assert!(mesh.extrude_face("f0", 1.0, "existing").is_err());
        assert_eq!(mesh, after_first);
    }

    #[test]
    fn concave_faces_triangulate_without_covering_the_concavity() {
        let mut mesh = empty_document();
        let polygon = [
            ("v0", [0.0, 0.0, 0.0]),
            ("v1", [2.0, 0.0, 0.0]),
            ("v2", [2.0, 2.0, 0.0]),
            ("v3", [1.0, 1.0, 0.0]),
            ("v4", [0.0, 2.0, 0.0]),
        ];
        for (id, position_m) in polygon {
            mesh.add_vertex(MeshVertex {
                id: id.to_owned(),
                position_m,
            })
            .unwrap();
        }
        for index in 0..polygon.len() {
            let next = (index + 1) % polygon.len();
            mesh.add_edge(MeshEdge {
                id: format!("e{index}"),
                vertices: [
                    polygon[index].0.to_owned(),
                    polygon[next].0.to_owned(),
                ],
            })
            .unwrap();
        }
        let face = MeshFace {
            id: "concave".to_owned(),
            vertices: polygon.iter().map(|(id, _)| (*id).to_owned()).collect(),
            material: None,
            uvs: None,
        };
        mesh.add_face(face.clone()).unwrap();

        let triangles = mesh.triangulate_face(&face).unwrap();
        assert_eq!(triangles.len(), 3);
        let triangulated_area = triangles
            .iter()
            .map(|triangle| {
                let a = mesh.vertices[triangle[0]].position_m;
                let b = mesh.vertices[triangle[1]].position_m;
                let c = mesh.vertices[triangle[2]].position_m;
                ((b[0] - a[0]) * (c[1] - a[1])
                    - (b[1] - a[1]) * (c[0] - a[0]))
                    .abs()
                    * 0.5
            })
            .sum::<f64>();
        assert!((triangulated_area - 3.0).abs() < 1.0e-9);
    }

    #[test]
    fn non_planar_faces_are_rejected_for_surface_triangulation() {
        let mut mesh = empty_document();
        for (id, position_m) in [
            ("v0", [0.0, 0.0, 0.0]),
            ("v1", [1.0, 0.0, 0.0]),
            ("v2", [1.0, 1.0, 0.2]),
            ("v3", [0.0, 1.0, 0.0]),
        ] {
            mesh.add_vertex(MeshVertex {
                id: id.to_owned(),
                position_m,
            })
            .unwrap();
        }
        for (index, vertices) in [
            ["v0", "v1"],
            ["v1", "v2"],
            ["v2", "v3"],
            ["v3", "v0"],
        ]
        .into_iter()
        .enumerate()
        {
            mesh.add_edge(MeshEdge {
                id: format!("e{index}"),
                vertices: vertices.map(str::to_owned),
            })
            .unwrap();
        }
        let face = MeshFace {
            id: "non_planar".to_owned(),
            vertices: ["v0", "v1", "v2", "v3"]
                .map(str::to_owned)
                .into(),
            material: None,
            uvs: None,
        };
        assert!(mesh.add_face(face).is_err());
    }

    #[test]
    fn mesh_document_updates_keep_a_previous_valid_generation() {
        let directory = temporary_directory("mesh-atomic-save");
        fs::create_dir_all(&directory).unwrap();
        let original = super::create_empty_mesh_document(&directory).unwrap();
        let path = directory.join(format!("{}.mesh.json", original.id));
        let mut updated = original.clone();
        updated
            .add_vertex(MeshVertex {
                id: "vertex_001".to_owned(),
                position_m: [1.0, 2.0, 3.0],
            })
            .unwrap();

        super::save_mesh_document_with_backup(&path, &original, &updated).unwrap();

        let current: MeshDocument =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let previous: MeshDocument = serde_json::from_slice(
            &fs::read(directory.join(format!("{}.mesh.json.prev", original.id))).unwrap(),
        )
        .unwrap();

        assert_eq!(current, updated);
        assert_eq!(previous, original);
        assert_eq!(discover_mesh_documents(&directory).unwrap(), vec![updated.clone()]);
        assert!(
            super::save_mesh_document_with_backup(&path, &original, &updated).is_err(),
            "a stale editor snapshot must not overwrite the newer document"
        );

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn missing_mesh_primary_is_restored_from_previous_generation() {
        let directory = temporary_directory("mesh-recovery");
        fs::create_dir_all(&directory).unwrap();
        let document = create_empty_mesh_document(&directory).unwrap();
        let primary = directory.join(format!("{}.mesh.json", document.id));
        let backup = directory.join(format!("{}.mesh.json.prev", document.id));
        fs::rename(&primary, &backup).unwrap();

        assert_eq!(discover_mesh_documents(&directory).unwrap(), vec![document]);
        assert!(primary.is_file());
        assert!(!backup.exists());

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn new_mesh_documents_are_persisted_and_discovered_without_overwriting() {
        let directory = temporary_directory("mesh-authoring");
        fs::create_dir_all(&directory).unwrap();

        let first = create_empty_mesh_document(&directory).unwrap();
        let second = create_empty_mesh_document(&directory).unwrap();
        assert_eq!(first.id, "mesh_001");
        assert_eq!(second.id, "mesh_002");
        assert_eq!(first.vertices.len(), 0);
        assert_eq!(first.edges.len(), 0);
        assert_eq!(first.faces.len(), 0);

        let discovered = discover_mesh_documents(&directory).unwrap();
        assert_eq!(discovered, vec![first, second]);

        fs::remove_dir_all(directory).unwrap();
    }
}
