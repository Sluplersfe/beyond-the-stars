# NEXT: Simple Features and Main Setup

Goal: a playable skeleton. A grid world you can look at, pan and zoom, place belt tiles on, and run at different speeds. No art, no UI framework, no saving yet.

Work in this order. Each step ends with `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` passing.

## 11. Sim: world and belts (`crates/sim`)

- [x] Add a `Belt` tile kind: `enum TileKind { Empty, Belt { dir: Dir } }` with `enum Dir { N, E, S, W }`. Derive `Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize`.
- [x] Store tiles in the existing `BTreeMap<TileId, Tile>`. Add `Sim::place_belt(x, y, dir) -> bool` (fails if occupied) and `Sim::remove(x, y)`.
- [x] Look up tiles by `(x, y)`: add a `BTreeMap<(i32, i32), TileId>` index, or compute `tile_id(seed, x, y)` directly.
- [x] Belts move items one tile per tick along `dir`. Items are a simple counter on each belt for now. Keep it integer-only.
- [x] Tests: placing on an occupied tile fails; a belt moves an item one tile per tick; the determinism hash still matches across runs.

## 12. Game: window and camera (`crates/game`)

- [x] Spawn a `Camera2d` in `Startup`.
- [x] Pan with WASD or arrow keys. Zoom with the mouse wheel. Keep these in `Update`.
- [x] Clamp zoom to a sane range (for example 0.25× to 4×).

## 13. Game: render the grid (`crates/game`)

- [x] Draw each sim tile as a coloured `Sprite` (empty = dark grey, belt = light grey with a direction marker). Mesh-per-tile is fine for now; batching comes later.
- [x] Sync from sim to visuals in `PostUpdate`. Rendering only reads sim state; it never writes to it.
- [x] Use integer tile coordinates. Convert to `f32` world position only at the render step.

## 14. Game: placing belts (`crates/game`)

- [x] Left-click an empty tile to place a belt in the current direction. Left-click an existing belt to remove it.
- [x] Press R to rotate the current direction.
- [x] Convert the mouse position to tile coordinates with the camera transform.
- [x] Input writes a command (`PlaceBelt`, `RemoveBelt`) into a queue. A system in `FixedUpdate` applies queued commands to `sim`. Input never calls `sim` directly.

## 15. Game: time controls (`crates/game`)

- [x] Space toggles pause. Pause means no fixed steps run.
- [x] Keys 1, 2, 3, 4 set speed to 1×, 4×, 16×, 64×. Speed is extra fixed steps per frame via `Time<Fixed>`.
- [x] Show the current speed and tick count as text (Bevy's built-in text is enough).

## 16. Main setup checks

- [x] `main.rs` only builds the app: plugins, resources, schedules. Put systems in modules (`camera.rs`, `render.rs`, `input.rs`, `time_control.rs`).
- [x] Keep `sim` free of Bevy. If a type needs a Bevy derive, wrap it in `game`.
- [x] Add the headless benchmark back in after step 11 with a belt-filled grid. Record the new number in section 10.

## Notes on sections 11-16

- Tiles live in a dense `Vec<Tile>` per planet surface (index `y * w + x`) instead of a `BTreeMap`; iteration order is still fixed, and belts are tracked in a sorted `BTreeSet` of tile indices. Building ids are still `tile_id(seed, x, y)` of the anchor.
- Belts carry one typed item per tile with a progress counter. Tiers: basic 4 ticks/tile, fast 2, express 1 (the "one tile per tick" test uses express belts).
- Keys 1-4 give 1x/4x/16x/64x, clamped to what research has unlocked; `+`/`-` step through 1x-1024x.
- Right-click also removes; left-clicking a belt with the belt tool removes it, as specified.

## 17. Full game vertical slice (done)

Everything below runs on the deterministic sim; the game crate only reads it and queues `Command`s.

- **Factory:** miners (infinite deposits, richness 1-3), smelters (auto recipe), assemblers, refineries, Mk2 generations, speed/efficiency modules, belts (3 tiers), inserters (2 tiers), depots and providers around a per-planet local inventory. Status markers show no power, missing inputs, full outputs, no workers.
- **Power:** generators (carbon/fuel), solar, fusion reactors (fuel cells). Unpowered machines stop and keep progress. Interplanetary, then interstellar, transmitters share surplus.
- **Economy:** 36 items across 6 tiers, 28 recipes.
- **Research:** 36 technologies in 7 branches, shared pool from staffed labs, switch freely with progress kept. Time-acceleration and Instant-Warp are in the tree.
- **Universe:** handcrafted home system (Terra Nova, Ferros, Glacia, Verdance), 3 galaxies of procedurally generated systems, generated lazily, with progression guarantees (gas at home, titanium in the home system, an exotic world near every galaxy core, void shards in other galaxies). Generation messages and physicist quotes appear only on first generation.
- **Exploration:** observatory scans (4 levels: detect, category, resource hints, rare/hostile), scout surveys, partial information until surveyed.
- **Ships and logistics:** scout/freighter/carrier hulls with chemical/warp/void drives, finite cargo and seats, fuel paid at departure, automated routes. Map view (M) at system, galaxy and universe scale shows ships in transit.
- **Colonies:** colony kits found colonies; work systems need the overseer present (ride a ship there). Houses keep people alive, population grows, ships need crew, labs/observatories/shipyards need workers, abandoned colonies stop human work.
- **Hostile life:** nests on living/volcanic/exotic worlds; turrets defend; per-planet Eliminate/Coexist policy.
- **Endgame:** the Frontier Beacon, built in another galaxy and powered for 60 s, wins (play continues).
- **Also:** save/load (F5/F9, `beyond-the-stars.save`), procedural sound effects (F2 mutes), help window with a goals checklist (F1).

- **3D presentation:** the planet view is a 3D low-poly scene (orbit camera: WASD pan, wheel zoom, middle-drag or `,`/`.` rotate) with procedural building models, animated belts, lava, sea and trees. System, galaxy and universe maps are 3D too (spiral galaxy cloud, glowing stars, orbits, ships).
- **Concept-style HUD:** top resource bar with +rate/min, Planet Overview, Selected Building card, minimaps, build toolbar (Mining ... Defense) with flyouts, Current Research card, galaxy side panels with Plan Journey, and the FTL & Time Control tech tree (2x to 32768x, then Instant-Warp, four tiers; U opens it).
- **Time control:** 16 speeds up to 32768x, gated by research. The sim steps inside a per-frame time budget, so slow machines drop the backlog instead of freezing.

Run: `cargo run -p game --release [seed]`.

Debug hooks (environment variables, see `crates/game/src/debug.rs`): `BTS_DEMO`, `BTS_VIEW` (system, galaxy, universe, research, techtree, logistics, colonies, help), `BTS_ZOOM`, `BTS_SCRIPT`, `BTS_SHOT`.

## Baseline (re-measured)

Headless benchmark, release build, 100x100 loop of 10,000 express belts with 5,000 items moving every tick, full `Sim::step`:

```
cargo run -p game --example headless_bench --release -- 1000
```

- Result: **~13,900 ticks/s** (`elapsed=0.072s`). 256x speed needs 5,120 ticks/s at 20 ticks/s base.
- Previous baseline (10,000 empty tiles, RNG only): ~26,600 ticks/s.

## Not done yet

Steam, art assets and music, belt lanes/splitters, terrain-adaptive and irregular footprints, balancing passes on recipe and research costs.

## Main menu, saves, settings, tutorial
- Main menu / pause menu (`menu.rs`), saves in `<data dir>/beyond-the-stars/saves` (`save.rs`, `BTS_HOME` overrides), settings JSON (`settings.rs`), 21-step tutorial (`tutorial.rs`, starts on New Game or from the menu).
- HTML background: `assets/menu/background.html` shown in a wry child webview (`webview.rs`, feature `html-menu`, Linux/X11). The menu is injected into the page (`menu_overlay.js`, `menu_web.rs`). Falls back to the native menu on Wayland/errors. Contract: `assets/menu/README.md`.
- The webview is hidden, never destroyed: destroying the child X window made GDK exit on BadWindow.
