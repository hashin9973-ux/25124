# Physics Sandbox

A data-driven, physics-first 3D sandbox foundation built with Rust, Bevy, and Avian.

The executable contains reusable engine systems. Game content is loaded from external folders:

```text
physics_sandbox.exe
data/       Engine settings, materials, and reusable component definitions
assets/     glTF scenes, textures, audio, and other streamed assets
mods/       Reserved for modular content extensions
worlds/     World compositions and per-world persistent state
```

## Foundation architecture

- **Definitions:** `data/components.json` describes reusable physical components.
- **Instances:** `worlds/<world>/world.json` places component definitions into a world using unique instance IDs and poses.
- **Materials:** `data/materials.json` defines intrinsic density and initial per-material friction/restitution coefficients.
- **Visual assets:** `assets/` contains models, textures, audio, and effects referenced by definitions.
- **Runtime state:** `worlds/<world>/state.json` stores saved instance transforms and velocities, separate from authored world composition.

This is the first definition/instance boundary. Nested assemblies, dependency-resolved content packages, hot reload, regional streaming, and runtime mod loading are intentionally future subsystems rather than partially implemented placeholders.

## Current foundation

- Bevy 0.20.1 with Avian 0.8.0.
- SI-style world scale: one world unit is one metre, velocities are metres/second, gravity is metres/second², and material density is kg/m³.
- Fixed 120 Hz physics schedule with eight solver substeps.
- Static and dynamic rigid bodies with shape-derived mass properties, data-defined density, static/dynamic friction, restitution, and per-component swept-CCD configuration.
- Cuboid, sphere, and capsule colliders. Primitive render meshes are generated from the same dimensions used by the colliders.
- Reusable mesh and primitive-material handles are shared for identical definitions in the loaded world.
- Avian collision-start/end counters plus an active-pair set synchronized from Avian's current touching-contact data.
- Optional glTF scene visuals loaded through Bevy's AssetServer. Imported scenes preserve their glTF-authored materials; primitive surface settings apply to engine-generated primitive visuals.
- Non-physical free-fly camera: W/A/S/D, Space/Ctrl, Shift, and mouse look.
- F5 saves instance transforms and linear/angular velocities. The state is loaded independently from authored configuration on the next launch.
- Save loading validates and normalizes quaternions and attempts backup recovery when the primary save is invalid.
- No AI, character, gameplay UI, or scenario-specific engine code.

## Run

Install a Rust toolchain supported by Bevy 0.20 and the operating-system dependencies required by Bevy, then:

```powershell
cargo run
```

Select another manifest:

```powershell
cargo run -- --world worlds/lab/world.json
```

Override portable roots when needed:

```powershell
cargo run -- --root . --data data --assets assets --world worlds/sandbox/world.json
```

The executable resolves its portable root from the directory beside the executable when that directory contains `data/`; development runs fall back to the current working directory.

## Controls

| Input | Action |
| --- | --- |
| W / A / S / D | Fly forward / left / backward / right |
| Space / Left Ctrl | Fly up / down |
| Left Shift | Sprint |
| Mouse | Look |
| F5 | Save current rigid-body state |

The camera is intentionally a non-physical editor/spectator camera; it is not a character controller and does not participate in collisions.

## Authoring content

- `data/engine.json`: versioned engine-level physics, camera, and lighting defaults.
- `data/materials.json`: versioned physical material catalogue. Density is kg/m³; friction coefficients are dimensionless.
- `data/components.json`: versioned reusable component definitions containing body type, collider, material reference, CCD setting, and visual reference.
- `worlds/<world>/world.json`: versioned world composition. Each object has a unique instance ID, a component-definition reference, and an SI transform.
- `assets/`: paths referenced by `visual.model` are relative to this directory. glTF scene selectors such as `models/part.glb#Scene0` are supported by the AssetServer.
- `worlds/<world>/state.json`: generated runtime state; it is not committed by default.

For a cuboid, `half_extents` describe the collision half-size in metres. The generated render mesh uses twice those values for its full dimensions. For imported models, the asset's authored origin and scale must agree with the collider definition; imported glTF materials are preserved. A future material-override pipeline will make model-level overrides data-configurable.

Current visual alpha modes are explicit: `opaque` requires alpha 1.0, while `blend` uses the authored alpha channel. Transparent materials can have different sorting and depth behavior from opaque materials.

The current friction values are approximations. Avian combines collider coefficients according to its configured combine rule; the foundation has not yet introduced a pairwise material-interaction registry. This is an intentional next physics-model extension, not a claim that friction is a universal material constant.

## Validation

```powershell
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

A Windows portable release can be assembled with:

```powershell
./scripts/package.ps1
```

The package script verifies Cargo's exit code and the executable before copying the portable directory layout. Mods remain a reserved directory in this foundation; runtime discovery and loading are later work.
