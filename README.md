# Physics Sandbox

A data-driven, physics-first 3D sandbox and character-authoring foundation built with Rust, Bevy, and Avian.

## Project layout

The executable contains reusable systems. World-specific content stays external:

```text
physics_sandbox.exe
data/                              Shared bootstrap definitions
assets/                            Shared project assets
mods/                              Reserved; runtime mod loading is not implemented
worlds/catalog.json                Loadable-world registry
worlds/sandbox/
  world.json                       Physics Sandbox manifest
  outputs/                         World-owned output location
worlds/character_studio/
  world.json                       Character Studio scene manifest
  assets/                          Studio-only models, textures, materials, references
  data/                             Studio-only engine, physical, editor, species and action data
  outputs/
    species/                       Reusable species-base definitions
    models/                        Exported model files
    actions/                       Action JSON
    previews/                      Preview renders
    autosaves/                     Workspace recovery data
```

The world catalog stores relative paths for each world's manifest, data, assets and outputs. A world can be loaded only when its catalog paths exist. The browser paginates the full catalog; Create World creates a separate world folder and copies the bootstrap engine/material/component configuration. Catalog updates are staged in the `worlds/` directory; the last valid generation is retained as `catalog.json.prev` so startup can recover when the primary catalog is missing or invalid.

## Main menu

The main menu and world browser use a procedural water surface with travelling waves and radial ripples. Pointer motion, clicks and periodic random impulses add ripples. Raised 3D text callouts have black offset geometry, white foreground text, stems, hover scaling and click actions. Worlds is active; Mods and Settings are intentionally present but inactive.

The water is currently a procedurally displaced, top-facing mesh with a low-roughness dielectric PBR material (water IOR 1.333), lit by a warm directional sun configured in lux. This is an initial visual implementation, not yet a full physically based ocean renderer: environment-cubemap reflections, refraction, screen-space reflections, caustics and a dedicated water shader are not implemented.

## Physics foundation

- Bevy 0.20.1 with Avian 0.8.0.
- SI-style scale: one world unit is one metre; velocity is metres/second; gravity is metres/second²; density is kg/m³.
- Fixed-step physics and solver substeps are loaded from the active world's engine configuration.
- Static and dynamic rigid bodies with cuboid, sphere and capsule colliders.
- Material definitions include density, static/dynamic friction, restitution, and linear/angular damping.
- The solver has a configurable low-speed restitution threshold. The rubber material uses reduced restitution and damping to avoid sustained low-speed bouncing.
- Directional-light orientation, illuminance and shadows update from the active world's configuration.
- F5 saves transforms and velocities for dynamic instances only. Static transforms remain manifest-owned; unresolved saved IDs are retained.

The free-fly editor camera is non-physical. It is not a character controller.

## Character Studio

Character Studio is a separate, world-owned authoring workspace. Its checked-in world manifest contains no instances, its species index is empty, and its action-set index is empty. Opening it does not spawn a model, rig, ragdoll, floor, workbench or reference object.

Choose **Create Mesh** in the Character Studio viewport to create a persisted, empty mesh document in the world's `outputs/models/` directory. Mesh documents store generic vertices, explicit edges, ordered polygon boundary loops, optional per-corner UV coordinates and optional material references. The modeling core validates references, duplicate topology, degenerate faces, simple planar polygon boundaries and finite geometry. Concave polygons are triangulated with ear clipping rather than a convex-only triangle fan.

The viewport renders the active mesh document over the configured construction-plane grid and supports ray-based vertex, edge and face picking. In Vertex mode, a click on the construction plane creates a vertex; hold Shift while clicking to build an ordered multi-selection. Press E with at least two selected vertices to create an edge between the last two selected vertices. Press F with at least three selected vertices to close their ordered loop into a face. Delete removes the current selection: deleting a vertex also removes incident edges and dependent faces, deleting an edge removes faces that use it, and deleting a face preserves its boundary topology. Arrow keys move the selected vertices, edges or faces by one configured construction-grid step along the plane; Page Up/Down move along its normal. The on-screen mode callouts and 1/2/3 shortcuts select Vertex, Edge and Face modes; Tab cycles loaded mesh documents. Each edit is staged, checked against the previously loaded on-disk document to reject stale writes, and preserves the previous generation as `.mesh.json.prev`. Startup restores that generation when an interrupted replacement leaves the primary file missing. This is optimistic version checking, not a cross-process file lock or a guarantee against every power-loss timing. Grid-step translation is available interactively; face extrusion remains programmatic and does not yet have a viewport control. Rotate, scale, undo/redo, and modifier evaluation are also incomplete.

Choose **Create Species** in the Character Studio viewport to create a persisted draft. The editor assigns a unique generic identifier, writes a species definition with no model or rig, and registers it in the studio's species index. It does not generate geometry, anatomical segments, joints, animations or actions. The draft is user-owned content and can be renamed as it is authored.

A species definition is reusable content. Runtime characters are separate instances derived from a user-authored species. Rig definitions, action sets, visual assets and physical material definitions remain independently authored and referenced; the engine does not prescribe anatomy, body-part names or movement patterns.

The studio-local rig loader validates user-authored segment and joint definitions. Its generic rigid-body preview supports sphere, capsule and cuboid segments, with physical materials, joint anchors, hinge axes and angular limits read from the referenced rig data. A preview is only created when a user explicitly selects a species with an authored rig through workspace configuration; the default workspace selects none.

The external editor tool registry describes the planned breadth of the studio, including geometry, mesh operations, sculpting, materials, rigging, animation, physics actions and export. The modeling core now supports persistent mesh documents, topology validation, vertex translation and face extrusion; interactive selection/editing, sculpting, animation and export remain incomplete. The action schema validator supports key presses, held keys, collisions, contact and surface geometry conditions, joint-angle and segment-rotation thresholds, and bounded joint torque or angular-velocity targets. A runtime action evaluator and physical actuator integration are not implemented yet. Soft-body runtime support is also not implemented.

## Run and controls

Install a Rust toolchain supported by Bevy 0.20 and the platform dependencies required by Bevy:

```powershell
cargo run --locked
```

The menu opens first. Choose Worlds and select a catalog entry. In a loaded world:
- W / A / S / D: free-fly camera movement
- Space / Left Ctrl: camera up/down
- Left Shift: sprint; Shift+click in Character Studio adds/removes mesh elements from the selection
- Mouse: look; left click in Character Studio selects elements or places vertices in Vertex mode
- Character Studio: 1 / 2 / 3 select Vertex / Edge / Face mode; arrow keys move selected topology by one configured grid step; Page Up/Down move along the construction-plane normal; E creates an edge; F creates a face; Delete removes topology with dependent-face cleanup; Ctrl+Z undoes and Ctrl+Y or Ctrl+Shift+Z redoes mesh edits; Tab cycles mesh documents. Undo history is per mesh document, session-local, and defaults to 64 steps. Configure `edit_history.maximum_undo_steps` in the world's modeling settings (0 disables history; the maximum is 256). Mesh documents themselves are saved after each edit; the undo stack is cleared when leaving Character Studio.
- F5: save the active world's dynamic body state
- Escape: return to the main menu

## Authoring and validation

Shared bootstrap data remains in `data/`. Per-world configuration is in `worlds/<world>/data/`; manifests are `worlds/<world>/world.json`; per-world assets are in `worlds/<world>/assets/`; and generated content belongs in `worlds/<world>/outputs/`.

Validation commands:

```powershell
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
```

A Windows portable release can be assembled with `./scripts/package.ps1`. The CI workflow packages and uploads `physics_sandbox-windows-x86_64.zip` as a workflow artifact.
