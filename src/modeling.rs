use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

const DOCUMENT_SCHEMA_VERSION: u32 = 1;
const GEOMETRY_EPSILON: f64 = 1.0e-12;

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
                vertex.position_m.iter().all(|value| value.is_finite()),
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
            vertex.position_m.iter().all(|value| value.is_finite()),
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
        let mut normal = [0.0_f64; 3];
        for index in 0..face.vertices.len() {
            let current = self.vertex_position(&face.vertices[index])?;
            let next = self.vertex_position(&face.vertices[(index + 1) % face.vertices.len()])?;

            normal[0] += (current[1] - next[1]) * (current[2] + next[2]);
            normal[1] += (current[2] - next[2]) * (current[0] + next[0]);
            normal[2] += (current[0] - next[0]) * (current[1] + next[1]);
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

    fn vertex_position(&self, id: &str) -> Result<[f64; 3]> {
        self.vertices
            .iter()
            .find(|vertex| vertex.id == id)
            .map(|vertex| vertex.position_m)
            .with_context(|| format!("unknown vertex '{id}'"))
    }
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
