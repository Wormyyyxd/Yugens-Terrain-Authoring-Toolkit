# Fork changes

This fork ([Wormyyyxd/Yugens-Terrain-Authoring-Toolkit](https://github.com/Wormyyyxd/Yugens-Terrain-Authoring-Toolkit))
tracks the `r` branch of [notnilcn/Yugens-Terrain-Authoring-Toolkit](https://github.com/notnilcn/Yugens-Terrain-Authoring-Toolkit)
and adds playtesting fixes on top of upstream `0602737` ("rust rewrite").

This file lists every change made in the fork, why it was made, and how it was checked. It is
updated in the same commit as each change.

Unless noted, measurements were taken on Windows 10 with Godot 4.7.1 (GL Compatibility), the
debug builds of the libraries, and the demo scene `demo/hex_island.tscn`.

## Summary

| Commit | Area | Change |
|---|---|---|
| `0666ba1` | Repo | Stop tracking Godot's `~*.dll` hot-reload copies |
| `c088305` | Demo player | Respawn on the last ground after falling off the map |
| `1551a88` | Weather / day-night | Keep fog enabled while a weather drives it (no rain hitch) |
| `3c2dc04` | Pixel renderer | Warm up effect shaders while the scene loads |

Each commit that touches Rust source also updates the matching prebuilt debug library in `bin/`,
because upstream ships those libraries and the demo runs from them without a Rust build.

---

## Stop tracking Godot's `~*.dll` hot-reload copies — `0666ba1`

**Problem.** Every launch of the editor or the game left three deleted files in `git status`:
`addons/MarchingSquaresTerrain/bin/~marching_squares_terrain.dll`,
`addons/phantom_camera_rust/bin/~phantom_camera_rust.dll` and `demo/bin/~mst_demo.dll`.

**Cause.** Godot copies each GDExtension library to a `~`-prefixed file for hot reloading and
deletes that copy on startup. Upstream had committed the three copies, so Godot deleted tracked
files on every run.

**Change.** Untracked the three copies and added `~*.dll` to `.gitignore`. The real libraries are
unchanged and still committed.

**Files.** `.gitignore`; the three `~*.dll` files removed from the index (they stay on disk).

---

## Respawn on the last ground after falling off the map — `c088305`

**Problem.** Jumping off the island into the ocean dropped the player below the map, where they
froze for good.

**Cause.** Falling below `fall_limit` (−40 m) calls `MstDemoPlayer::spawn()`, which samples the
terrain height directly under the player. Off the island there is no terrain, so the sample is
NaN; `spawn()` treats that as "terrain not loaded yet" and holds the body still (no gravity)
until it is. That never happens out there.

**Change.** The player remembers the last position where it stood on the floor (or the scene's
start position if it never landed) and moves there before re-spawning, so the height sample hits
terrain.

**Files.** `rust/crates/mst_demo/src/player.rs`; rebuilt `demo/bin/mst_demo.dll`.

**Verified.** Headless script: land, teleport to (300, −50, 300) over the ocean, step 30 physics
frames. The original library left the player at (300, −50, 300); the fixed one put it back on
the ground at its last position.

---

## Keep fog enabled while a weather drives it — `1551a88`

**Problem.** The first time it rained (also Foggy and Storm), the game froze for 100–180 ms.

**Cause.** `PixelDayNight` (and `PixelWeather` when there is no day-night node) switched the
Environment's fog **on** when a weather brought fog and **off** when it cleared. In the
Compatibility renderer, fog on/off is a shader specialization, so turning it on recompiled every
visible material in a single frame. Found by elimination: timers in the fire system never
exceeded 3 ms, the hitch happened with rain alone, and a load-time shader warm-up did not remove
it.

**Change.** While a weather node exists, fog stays enabled and only its density changes (0 = no
fog). The background counts as infinitely far away, so even zero-density fog would paint the
clear-colour sky with the fog colour. `fog_sky_affect` is therefore held at 0 while there is no
fog and restored to the environment's own value when fog arrives (and when the weather goes
away).

**Files.** `addons/MarchingSquaresTerrain/rust/crates/mst/src/effects/day_night.rs`,
`.../effects/weather.rs`.

**Verified.**
- Cold shader cache: first rain went from 176–178 ms to 8.5–10 ms.
- Rendered frames (Movie Maker, fixed 30 fps) compared pixel by pixel with the previous build:
  clear weather frame 30 and rain frames 30 and 85 are identical. Later clear frames differ only
  where birds and clouds move; two runs of the *original* build differ far more than that.

---

## Warm up effect shaders while the scene loads — `3c2dc04`

**Problem.** Smaller hitches the first time an effect appeared: rain splashes, snow, fire
flames, smoke and steam, and fire lights reaching grass and terrain.

**Cause.** The Compatibility renderer compiles a material's shader variant the first time it is
drawn. These effects are hidden or off-screen when the scene starts, so they compiled mid-game.
Godot then caches compiled shaders on disk, so the hitches mostly happen on a machine's first
play and fade on later runs.

**Change.** New `render/warmup.rs`, driven by `PixelRenderer` through a new export
`warm_up_shaders` (on by default, game only). Three frames after start it collects every
material of every `GeometryInstance3D` in the viewport (hidden and internal nodes included) and
draws each for two frames on a sub-pixel proxy in front of the camera:

- MultiMesh proxies with the source's instance format for MultiMesh sources (the instanced
  variant is a separate compile), plain quads otherwise;
- the source's render layers and shadow casting, so every pass of the pixel pipeline (colour,
  meta, id, shadows) compiles;
- a faint omni light over the proxies, so the omni-lit variant compiles too (fire lights are
  created mid-game).

The proxies are freed afterwards. In the demo it warms 34 material variants.

**Files.** `addons/MarchingSquaresTerrain/rust/crates/mst/src/render/warmup.rs` (new),
`.../render/mod.rs`, `.../render/renderer.rs`; rebuilt
`addons/MarchingSquaresTerrain/bin/marching_squares_terrain.dll` (this build also contains the
fog change above).

**Verified.** Scripted run with the game's shader cache deleted first: ignite a grass fire, then
Rain, Snow, Storm, Clear and reset, twice. Worst frame 178 ms before (first rain), 16 ms after
both changes. Lighting a fire in the rain: 32 ms → 9 ms.

---

## Known upstream issues not changed

- `.gitmodules` still declares `addons/phantom_camera_rust` and `addons/MarchingSquaresTerrain`
  as submodules, but on `r` they are ordinary directories. Do not run
  `git submodule update --init`.
- `CREDITS.md` is referenced by the addon README but only exists on `master`.
