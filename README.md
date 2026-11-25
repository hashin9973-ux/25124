# Physics Sandbox

A data-driven, physics-first 3D sandbox foundation built with Rust, Bevy, and Avian.

The executable contains reusable engine systems. Game content is loaded from external folders:

```text
physics_sandbox.exe
data/       Engine and physical-material definitions
assets/     glTF scenes, textures, audio, and other streamed assets
mods/       Reserved for modular content extensions
worlds/     World manifests and per-world persistent state
```

## Current foundation

- Bevy 0.20.1 with Avian 0.8.0 (the upstream-compatible release pair).
- SI-style world scale: one world unit is one metre, velocities are metres/second, gravity is metres/second², and material density is kg/m³.
- Fixed 120 Hz physics schedule with eight solver substeps.
- Collider dimensions, primitive render meshes, body types, transforms, and material references are authored in world/material JSON—not in scenario-specific Rust systems.
- Static and dynamic rigid bodies with shape-derived mass properties, per-material density, static/dynamic friction, and restitution.
- Avian contact-start/contact-end messages collected into a public ECS contact ledger.
- Optional glTF scene visuals loaded through Bevy's AssetServer. Collider geometry remains explicit world data; author model scale/origin to match its collider.
- Free-fly camera: W/A/S/D, Space/Ctrl, Shift, and mouse look.
- F5 saves body transforms and linear/angular velocities to the selected world's `state.json`. The save file is loaded on the next launch. Writes use a temporary file and backup/restore to protect an existing save from an interrupted replacement.
- No AI, character, gameplay UI, or scenario-specific engine code.

## Run

Install a Rust toolchain supported by Bevy 0.20, then:

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

- `data/engine.json`: engine-level physics, camera, and lighting defaults.
- `data/materials.json`: physical material catalogue. Friction coefficients are dimensionless Coulomb coefficients; density is kg/m³.
- `worlds/<world>/world.json`: object list, body type, SI transform, collider shape, material reference, visual tint, and optional glTF scene path.
- `assets/`: paths referenced by `visual.model` are relative to this directory. glTF scene selectors such as `models/part.glb#Scene0` are supported by the AssetServer.
- `worlds/<world>/state.json`: generated runtime state; it is not committed by default.

For a cuboid, `half_extents` describe the collision half-size in metres. The procedural render mesh is generated from the same values, so its dimensions and collider dimensions match exactly. For imported models, the asset's authored origin and scale must agree with the collider definition. Do not rely on visual mesh bounds to silently invent physical dimensions.

Material friction values are approximations for a material pair, not immutable universal constants. Avian combines the two surfaces' coefficients according to its combine rule; this foundation explicitly supplies both static and dynamic coefficients rather than relying on engine defaults.

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

The package script places the release executable beside `data/`, `assets/`, `mods/`, and `worlds/`. Mods are a reserved directory in this first slice; runtime mod discovery/loading is a later subsystem.
