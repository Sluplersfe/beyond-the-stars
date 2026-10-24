# NOW: Foundation Setup

Goal: a Cargo workspace with a deterministic simulation crate and a Bevy app crate, plus a headless benchmark. Nothing game-specific yet.

## 1. Toolchain

Bevy 0.20.0 (latest stable on crates.io) needs **Rust 1.97.1 or newer**. This machine has rustc 1.95.0, so update first:

```bash
rustup update stable
rustc --version   # must be >= 1.97.1
```

Linux system packages for Bevy (Debian/Ubuntu names; use your distro's equivalents):

```bash
sudo apt install -y pkg-config libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev
```

Vulkan drivers must be installed for wgpu to render (`mesa-vulkan-drivers` on Debian/Ubuntu).

## 2. Workspace layout

```
beyond-the-stars/
├── Cargo.toml          # workspace root
├── now.md              # this file
├── crates/
│   ├── sim/            # simulation core: NO bevy rendering deps
│   └── game/           # Bevy app: window, rendering, input, UI
└── benches/            # headless benchmark scenes (added at milestone 2)
```

Create the workspace root `Cargo.toml`:

```toml
[workspace]
resolver = "3"
members = ["crates/sim", "crates/game"]

[workspace.package]
edition = "2024"
rust-version = "1.97.1"
license = "UNLICENSED"

[workspace.dependencies]
bevy = "0.20"
serde = { version = "1", features = ["derive"] }
rmp-serde = "1"

# Fast iteration: optimise dependencies even in dev builds.
[profile.dev.package."*"]
opt-level = 3

[profile.release]
lto = "thin"
codegen-units = 1
```

Create the crates:

```bash
cargo new --lib crates/sim --name sim
cargo new crates/game --name game
```

## 3. Simulation crate (`crates/sim`)

Rules for this crate:
- **No `bevy` dependency** in `sim/Cargo.toml` by default. Bevy is used only through `game`. If the sim needs ECS, use `bevy_ecs` alone, and decide that explicitly in an ADR before adding it.
- **Fixed-point math** for all simulation state. No `f32`/`f64` in anything that affects game state. Floats are allowed for rendering only.
- **Seeded RNG only.** Use `rand_pcg` or `rand_xoshiro`. Never `thread_rng`.
- **Stable IDs.** Entities that persist get IDs derived from seed + coordinates, not from allocation order.
- **No iteration over `HashMap` for state updates.** Use `BTreeMap` or sorted `Vec`, so results don't depend on hash order.

`crates/sim/Cargo.toml` dependencies to start:

```toml
[dependencies]
serde = { workspace = true }
rmp-serde = { workspace = true }
```

## 4. Game crate (`crates/game`)

`crates/game/Cargo.toml`:

```toml
[dependencies]
bevy = { workspace = true }
sim = { path = "../sim" }
```

Start with a window and a fixed-timestep schedule that calls into `sim`:

- Run simulation logic in `FixedUpdate`, not `Update`. Time multipliers (2×, 4×, … 256×) are just more fixed steps per frame via `Time<Fixed>`.
- Pause = no fixed steps run.
- Rendering and input stay in `Update`/`PostUpdate`. Never mutate sim state from rendering code.

Check the `bevy` feature list before the first build. Start with defaults; trim later for compile time.

## 5. Headless benchmark (milestone 2 prep)

The sim must run without a window. In `crates/game` or a `benches/` crate:

- Build a Bevy `App` with `MinimalPlugins` (no window, no renderer).
- Spawn a fixed scene (for example 10,000 belt tiles).
- Step `FixedUpdate` N times and report ticks per second.

Record the baseline in this file once it exists. This number is the first real performance target.

## 6. Tests

- `cargo test -p sim` must pass before any Bevy code is added on top.
- **Determinism test:** run the same seed for K ticks twice and assert identical state hashes. This test is mandatory from the first sim system.
- Keep unit tests next to the code they cover.

## 7. Checks before commit

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p game
```

## 8. Deferred (do not add yet)

- **Steam:** `steamworks` (current: 0.13.1). Add when there is a playable build.
- **UI:** `bevy_egui` (current: 0.43.1) for debug tools only. Before adding it, check its compatibility table against Bevy 0.20; the versions may not match yet. Shipping UI decision is open.
- **Audio, persistence I/O, procedural generation:** add with the milestone that needs them.

## 9. Open items

- [x] Run `rustup update stable` and confirm rustc ≥ 1.97.1. (rustc 1.99.1)
- [x] Create the workspace and both crates. (Bevy 0.20.0 resolved)
- [x] Get `cargo build -p game` to open an empty window. (Builds; runs 8s on X11 without error. Not visually checked.)
- [x] Add the first determinism test in `sim`. (`determinism_same_seed_same_hash`)
- [x] Add the headless benchmark scaffold. (`crates/game/examples/headless_bench.rs`)
- [x] Commit to git. (`git init` done; first commit `7707b20`)

## 10. Baseline

Headless benchmark, release build, 10,000 tiles, 1,000 ticks:

```
cargo run -p game --example headless_bench --release -- 1000
```

- Result: **~26,600 ticks/s** (`elapsed=0.038s`, Pcg64Mcg, 16.16 fixed-point)
- This is the first performance target. Re-measure after changes.
