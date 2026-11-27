# Physics Sandbox

A data-driven 3D rigid-body and voxel sandbox built with Rust, Bevy 0.20.1, and Avian 0.8.0. The executable hosts reusable simulation systems; the simulation, physical materials, object definitions, world composition, chunk data, and menu layout live outside `src/`.

## Portable project layout

```text
physics_sandbox.exe
data/
  engine.json                 Fixed-step solver, sleeping, camera, lighting
  materials.json              SI density, friction and restitution catalog
  components.json             Reusable collider, visual and damage definitions
assets/
  ui/main_menu.json           Main-menu text, layout, colors and actions
  models/ ...                 Optional glTF models and textures
mods/                         Reserved for content packages
worlds/
  <world>/
    world.json                World metadata and region references
    regions/
      <region>.json           Region index and chunk references
      chunks/
        <chunk>.json          Palette-indexed voxel cells
    state.json                Generated runtime save, not authored world content
```

The Rust source owns systems and validation, not scenario-specific visuals or UI markup. New physical objects are defined in `data/components.json`, with physical properties referencing `data/materials.json`. `assets/ui/main_menu.json` controls the menu independently of source code.

## World library

Launching without --world opens the world library. It discovers every world.json recursively beneath worlds/ and lists discovered worlds in stable name/ID order. Selecting an entry validates that world's manifest, region index and chunk metadata, then restores that world's own state.json. No world simulation is spawned before selection. Add a new world by creating a folder with a valid world.json; the library does not require source-code registration or a hard-coded scenario list.

Passing --world <path> remains available for direct startup and development automation. It bypasses the library and starts the specified world.
## Simulation

- SI-style scale: one world unit is one metre, linear velocities are m/s, angular velocities are rad/s, gravity is m/s², and density is kg/m³.
- Fixed 120 Hz physics with eight solver substeps.
- Cuboid, sphere and capsule colliders; collider and primitive visual dimensions come from the same definitions.
- Density-derived mass properties, static/dynamic friction, restitution, and optional per-definition swept CCD.
- Four restitution solver iterations for multi-point contacts.
- A configurable 0.5 m/s maximum overlap-correction speed limits velocity introduced by numerical penetration correction. This is distinct from impact velocity and does not overwrite a body's momentum.
- Restitution threshold is configured explicitly (1.0 m/s in the sample). Avian treats lower-speed contacts as inelastic to prevent contact jitter; tune this per project if the sample is too dissipative for a chosen material.
- Avian island sleeping is enabled with configurable linear/angular thresholds (0.15 m/s and 0.15 rad/s in the sample) and a 0.5 s rest interval. Bodies are not manually zeroed: resting islands enter the engine's sleeping state, and contact/force changes wake them.
- Contact-start/end counters and a live contact-pair ledger are available to other engine systems.
- Material-specific rolling resistance is applied as a contact impulse only while supported by a contact normal; it does not add global angular damping.
- Camera is a non-physical spectator/editor camera (W/A/S/D, Space/Ctrl, Shift, mouse look). It pauses with the main menu.

The solver settings are external in `data/engine.json`. Restitution belongs to the material definition; it is not a constant baked into source. Avian describes restitution as an approximation: sequential contact solving, friction, speculative collision, and multiple contact points affect the energy response. This is a numerical rigid-body simulation, not a claim of exact continuum-material physics.

## Destructibility

Any cuboid component definition may contain an optional `destructibility` block:

- `impact_speed_threshold_m_s`: minimum approaching contact-normal speed that triggers fracture.
- `fragments_per_axis`: number of pieces along X/Y/Z (each 2–4; no more than 64 total).

The fracture system observes Avian's pre-solver contact-normal speed and contact impulse. When the threshold is crossed, it replaces the original cuboid with a grid of dynamic fragments. Fragment dimensions tile the original collider volume; fragments inherit the original rigid body's linear/angular velocity field. A fractured static block receives the measured contact impulse when its fragments are spawned. Damage and fragment generation are therefore data-driven rather than tied to object names.

A component may define bond_strength with elastic modulus, separate tensile/compressive strengths, shear, bending and torsional limits in pascals, and a progressive-damage rate. Adjacent authored voxels with bond definitions are connected by fixed joints. Avian joint force and torque are converted to nominal face stresses. A matrix-free, diagonally preconditioned conjugate-gradient truss solve estimates gravity-driven axial load sharing through connected paths that reach static supports. Multiple paths share load by member stiffness; after a member fails, the next solve redistributes its load to the surviving graph. Damage accumulates above 50% utilization and is persisted in save format 2. Existing version-1 saves are upgraded on load.

Current constraints are deliberate and validated: impact-fracture targets must use cuboid colliders and procedural primitive visuals. Imported glTF fracture geometry is not yet implemented. Fragment pieces remain rigid bodies. The structural network is a small-displacement, linear-elastic axial truss approximation under gravity. Avian remains authoritative for rigid-body motion and local joint/contact response; the truss does not model continuum strain, plasticity, buckling, shear/bending redistribution, or arbitrary external load cases. These limitations are explicit rather than claiming finite-element accuracy.

## Voxel world format

World data is a hierarchy rather than a monolithic world JSON:

1. `world.json` selects one or more region manifests.
2. A region manifest indexes chunk IDs, coordinates and relative chunk paths.
3. Each chunk declares dimensions, voxel size, a block-definition palette, and sparse Y layers. Each row stores one hexadecimal palette index per X cell; rows are ordered along Z. Palette index `0` is air.
4. Non-air palette entries reference cuboid component definitions in `data/components.json`; voxel dimensions scale from the referenced collider dimensions using the chunk's `voxel_size_m`.

The sample overworld contains four 16×16×16 chunks. All chunks in one world share dimensions and voxel size so integer chunk coordinates map to one consistent voxel lattice. The chunk coordinate, dimensions and voxel size determine its world-space origin. Each occupied voxel receives a stable ID based on chunk ID and cell coordinates, so runtime transforms, fracture state and saves can refer to a block independently of its neighbours.

Startup reads region indexes and spatial metadata only. Voxel payloads are read asynchronously when the spectator camera approaches, with configured limits for parallel file reads and per-frame spawns. Authored `world.objects` are also partitioned into deterministic spatial object chunks using `object_chunk_size_m`; they enter the same residency scheduler and share its spawn budget. Both chunk types use load/unload radii with hysteresis. Before unload, dynamic body transforms/velocities and detached-fragment state are preserved; chunk-owned joints are removed with their endpoint entities, and broken bonds/fractures remain journaled.

Voxel rendering has a separate mask from physics batching. Intact static cells—including destructible and bonded concrete platform cells—participate in visual LOD, while only non-interactive static cells may merge into a compound collider. Dynamic bodies, fractured cells, imported-model visuals, and blended visuals are never folded into those batches. Optional visual LOD is configured in `data/engine.json`: full-resolution LOD0, factor-two LOD1, factor-four LOD2, and factor-eight LOD3 are generated where chunk dimensions permit. Transitions use distance to the chunk bounds, not just its center. Proxies are render-only; exact LOD0 colliders remain active. If a voxel in a chunk fractures, its coarse snapshot is hidden so the chunk shows exact surviving cells rather than a stale proxy. Authored object chunks currently stream at their authored mesh detail; coarse object-proxy generation is not yet implemented. Bond reconnection uses stable voxel identities across chunk boundaries, so loading order does not create duplicate joints. Chunk editing/transactional authoring and seam-aware cross-chunk face merging remain future work.

## Menu and controls

The main menu is loaded from `assets/ui/main_menu.json`. It configures title/subtitle, colors, dimensions, button labels, and actions. Supported actions are `start`, `resume`, and `quit`; the UI system remains in `src/ui.rs`, while all presentation values are external. Press **Escape** to toggle the menu; simulation time and spectator-camera input pause while it is visible.

| Input | Action |
| --- | --- |
| W / A / S / D | Fly forward / left / backward / right |
| Space / Left Ctrl | Fly up / down |
| Left Shift | Sprint |
| Mouse | Look |
| Escape | Open/close main menu |
| F5 | Save rigid-body state, fracture IDs, broken bonds and fragment state |

The camera is intentionally non-physical. It is not a player character and does not participate in collisions. World-library startup shows the selectable world collection first; Escape toggles the in-world menu while a simulation is active.

## Authoring

- `data/engine.json`: physics solver, sleeping, camera and rendering configuration. SI units are encoded in field names.
- `data/materials.json`: density in kg/m³ and dimensionless friction/restitution coefficients.
- `data/components.json`: reusable body, collider, material, swept-CCD, destructibility and visual definitions.
- `worlds/<world>/world.json`: versioned world metadata, region references and optional gravity override.
- `worlds/<world>/regions/*.json`: region indexes.
- `worlds/<world>/regions/chunks/*.json`: chunk voxel data.
- `assets/ui/main_menu.json`: menu presentation and supported actions.
- `assets/`: model and texture paths referenced by `visual.model` are relative to this directory. glTF scene selectors such as `models/part.glb#Scene0` are supported.

Example CLI:

```powershell
cargo run --locked
cargo run -- --world worlds/lab/world.json
cargo run -- --root . --data data --assets assets --world worlds/sandbox/world.json
```

Paths may be absolute or relative to `--root`. The executable resolves its portable root from the directory beside the executable when that directory contains `data/`; development runs fall back to the current working directory.

F5 writes `state.json` atomically and keeps backup/recovery generations. It stores dynamic object transforms and velocities, fractured instance IDs, progressive bond damage, broken bonds, and live fragment transforms/velocities. Save format 2 migrates version-1 files on load. Authored static transforms remain in manifests/chunks. Unresolved object records are retained to support content that is temporarily unavailable.

## Local validation

No GitHub-hosted CI workflow is enabled. Run the repository checks on the local development machine with:

```powershell
./scripts/validate.ps1
```

The script checks formatting, type-checks every target, runs all tests and runs Clippy with warnings denied. To omit the lint pass:

```powershell
./scripts/validate.ps1 -SkipClippy
```

The equivalent individual commands are:

```powershell
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
```

The committed `Cargo.lock` pins the dependency graph. Keep it under version control and update it deliberately.

A Windows x64 portable release can be assembled from a Windows machine with the MSVC C++ build tools and Windows SDK installed:

```powershell
./scripts/package.ps1
```

The script builds `x86_64-pc-windows-msvc` in Release mode with a statically linked MSVC C runtime, then creates:

- `dist/physics-sandbox-windows-x64/` — unpacked portable folder.
- `dist/physics-sandbox-windows-x64.zip` — distributable archive.
- `dist/physics-sandbox-windows-x64.zip.sha256` — SHA-256 checksum.

The package contains `physics_sandbox.exe`, a `Physics Sandbox.cmd` launcher, portable instructions, and the complete `data/`, `assets/`, optional `mods/`, and `worlds/` trees. Windows and compatible GPU drivers remain host requirements.

The `Portable Windows Build` GitHub Actions workflow runs formatting, locked checks, tests, Clippy, package creation, and archive-content verification, then uploads the ZIP and checksum as a workflow artifact.
