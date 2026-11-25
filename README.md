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

The world catalog stores relative paths for each world's manifest, data, assets and outputs. A world can be loaded only when its catalog paths exist. The browser paginates the full catalog; Create World creates a separate world folder and copies the bootstrap engine/material/component configuration.

## Main menu

The main menu and world browser use a procedural water surface with travelling waves and radial ripples. Pointer motion, clicks and periodic random impulses add ripples. Raised 3D text callouts have black offset geometry, white foreground text, stems, hover scaling and click actions. Worlds is active; Mods and Settings are intentionally present but inactive.

The water is currently a procedurally displaced mesh shaded by a PBR material and the project's directional light. This is an initial visual implementation, not yet a physically based ocean renderer: environment-cubemap reflections, refraction, screen-space reflections, caustics and a dedicated water shader are not implemented.

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

Character Studio is a separate catalog world. Its `assets/`, `data/` and `outputs/` are world-local, so adding another editor world does not require mixing that world's textures, model files, species definitions or actions with this one.

A **species** is a reusable base definition (the checked-in example is `human`), not an individual character. The species record links to a rig and action sets. Runtime individuals are intended to be instances derived from a species.

The sample rig defines articulated rigid-body segments and joints. It explicitly does not use rotation locks or translation locks. The action JSON describes:
- input triggers such as a held key;
- conditions such as foot/surface contact and joint-angle thresholds;
- collision-triggered reflexes;
- joint torque and bounded joint-angular-velocity actuators.

The execution contract disables impulse jumps, root teleportation, rotation locks and external-object push movement. The action schema is data only at this stage: the runtime evaluator that turns triggers into joint motor forces is not implemented yet. Soft-body runtime support is also not implemented.

The external editor tool registry records the planned breadth of the studio (mesh edit, sculpt, surface/topology, PBR materials, rigging, animation, physics actions and export). Entries marked `specification_only` are design contracts—not working operators. A human GLB has not been authored yet; `outputs/models/human.glb` is the intended export target, not an existing asset.

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
