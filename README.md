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

Choose **Create Species** in the Character Studio viewport to create a persisted draft. The editor assigns a unique generic identifier, writes a species definition with no model or rig, and registers it in the studio's species index. It does not generate geometry, anatomical segments, joints, animations or actions. The draft is user-owned content and can be renamed as it is authored.

A species definition is reusable content. Runtime characters are separate instances derived from a user-authored species. Rig definitions, action sets, visual assets and physical material definitions remain independently authored and referenced; the engine does not prescribe anatomy, body-part names or movement patterns.

The studio-local rig loader validates user-authored segment and joint definitions. Its generic rigid-body preview supports sphere, capsule and cuboid segments, with physical materials, joint anchors, hinge axes and angular limits read from the referenced rig data. A preview is only created when a user explicitly selects a species with an authored rig through workspace configuration; the default workspace selects none.

The external editor tool registry describes the planned breadth of the studio, including geometry, mesh operations, sculpting, materials, rigging, animation, physics actions and export. The modeling, sculpting, animation and export groups are specifications rather than completed operators. The action schema validator supports key presses, held keys, collisions, contact and surface geometry conditions, joint-angle and segment-rotation thresholds, and bounded joint torque or angular-velocity targets. A runtime action evaluator and physical actuator integration are not implemented yet. Soft-body runtime support is also not implemented.

## Run and controls

Install a Rust toolchain supported by Bevy 0.20 and the platform dependencies required by Bevy:

```powershell
cargo run --locked
```

The menu opens first. Choose Worlds and select a catalog entry. In a loaded world:
- W / A / S / D: free-fly camera movement
- Space / Left Ctrl: camera up/down
- Left Shift: sprint
- Mouse: look
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
