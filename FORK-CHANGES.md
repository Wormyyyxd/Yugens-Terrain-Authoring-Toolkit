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
| `9518435` | Docs | Add this file |
| `3a5c3ae` | Demo scene | Fewer fireflies at the camp |
| `d0d21c8` | Terrain shaders | Wet ground keeps its colour instead of turning grey |
| `9546014` | Terrain LOD | Spread distant-LOD chunk rebuilds over frames (no orbit stutter) |
| `15d3a0f` | Pixel renderer | Warm up the shadow-off variant too (no freeze at the first night) |
| ~~`596d005`~~ | Weather | ~~Thin weather fog as a third-person camera zooms out~~ — **reverted** by the next row |
| `857a984` | Terrain LOD | Keep chunks at full detail for 3 s; keep collision when dropping detail |
| `0027345` | Weather | Revert `596d005` (it moved the grey problem around instead of fixing it) |
| `d0011ab` | Pixel renderer | Draw fog after the palette, so fogged things keep their colour |
| `fefa2f6` | Grass / props | Grass and trees sway with the weather's wind |
| `c3086f0` | Pixel renderer | Add god rays after the palette, so they actually show |
| `f9989a2` | Sky | Keep the cloud deck above the camera (no smeared / covering clouds when zoomed out high) |
| *(next)* | Sky | Keep the camera inside the cloud deck (clouds vanished when tilting up close) |

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

## Fewer fireflies at the camp — `3a5c3ae`

**Problem.** Playtest feedback: the fireflies around the camp at night were a little too dense.

**Change.** `Fireflies.count` in the demo scene from 64 to 40 (the `PixelFireflies` default is
48), over the same 18 × 18 m area. Pure tuning; the addon is unchanged. The count can be tried
live in the F3 tuner (Effects → Fireflies).

**Files.** `demo/hex_island.tscn`.

---

## Wet ground keeps its colour — `d0d21c8`

**Problem.** In rain, grass-covered ground and paths turned grey-purple instead of a darker,
wetter version of themselves.

**Cause.** Measured on the renderer's Raw Color debug view (before palette snapping), comparing
the same pixels dry and wet:

- Ground wetness was a plain multiply with a blue tint, `× 0.66 × (0.9, 0.96, 1.06)` in
  `pt_weather_surface`, and the fire system's wet-patch layer multiplied again by
  `(0.8, 0.86, 0.9)`. The fire field is wet everywhere while it rains, so both always applied:
  red × 0.48, green × 0.55, blue × 0.63. That cancels the warmth of sand, dirt and paths: a tan
  path `(222,189,142)` became neutral grey `(101,100,88)` from wetness alone, which the palette
  then snapped to its grey ramp.
- The Rain preset's blue-grey fog (65 % mix) greys distant colours further. That is the
  intended rainy mood and was left alone.

**Change.**
- `pt_weather_surface`: wet ground is darkened to 66 % *and* its saturation raised by up to
  35 % (real wet ground looks darker and more saturated); no blue tint.
- `pt_fire_surface`: the wet-patch darkening (now `× 0.84`, hue kept) fades out as
  `pt_wet` rises, so ground the weather already wet is not darkened twice. Doused patches in dry
  weather still show.
- `pixel_weather.gdshaderinc` defines `PT_WEATHER`; `pixel_fire.gdshaderinc` only reads `pt_wet`
  when that is defined, so shaders that include `pixel_fire` without `pixel_weather` still
  compile.

**Files.** `addons/MarchingSquaresTerrain/shaders/pixel_weather.gdshaderinc`,
`addons/MarchingSquaresTerrain/shaders/pixel_fire.gdshaderinc` (shaders only, no rebuild needed).

**Verified.** Raw colour, averaged over the same ground pixels:

| | Tan path | Green ground |
|---|---|---|
| Dry | (222,189,142) | (127,183,80) |
| Wet only, before | (101,100,88) | (63,109,54) |
| Wet only, after | (150,122,80) | (76,127,44) |
| Full Rain, before | (97,101,102) | (80,107,88) |
| Full Rain, after | (123,112,98) | (86,116,84) |

In the final image, foreground grass ground stays green and dirt turns a wet brown. No shader
compile errors.

---

## Spread distant-LOD chunk rebuilds over frames — `9546014`

**Problem.** Zoomed fully out (100 m), turning the camera quickly stuttered.

**Cause.** Not rendering and not the camera (it moved smoothly every frame). The terrain's
distant LOD re-evaluates every 0.2 s which chunks get the low-detail mesh, keeping full detail
along the segment from the LOD focus (the player) to the camera. At 13 m that segment barely
moves; at 100 m an orbiting camera sweeps it across the island, so each tick flipped 20–56 of
the 295 chunks between low and full detail. All of them were rebuilt (mesh, collision, grass
regions; about 1 ms each, measured with the crate's `MST_PROF` scopes) in one frame: a 25–55 ms
hitch five times a second.

**Change.** `set_lod_chunks` queues flipped chunks in `lod_pending` instead of marking them
dirty. Each frame `drain_lod_pending` moves at most `LOD_REBUILDS_PER_FRAME` (6) of them,
nearest to the LOD focus first, into the dirty set. A chunk that flips back before it was
rebuilt leaves the queue (its mesh is right again), and chunks dirtied for any other reason
(edits, reloads) are rebuilt immediately as before.

**Files.** `addons/MarchingSquaresTerrain/rust/crates/mst/src/terrain/node.rs`; rebuilt
`addons/MarchingSquaresTerrain/bin/marching_squares_terrain.dll`.

**Verified.** Scripted orbit with real input events (arrow-key action), 5 s each, two runs:

| | Before | After |
|---|---|---|
| Orbit at 13 m, worst frame | 18–19 ms | 15.5–17 ms |
| Orbit at 100 m, worst frame | 54–55 ms | 17.4–17.6 ms |
| Frames over 25 ms at 100 m | 10–11 | 0 |
| Snap zoom 13 → 100 m | 56–57 ms | under 25 ms |

---

## Warm up the shadow-off variant too — `15d3a0f`

**Problem.** The first switch to night (F3 → Day/Night → Night, or the clock reaching dusk)
froze the game.

**Cause.** `PixelDayNight` switches the sun's shadow **off** below `DUSK_FLOOR` (the moon casts
none). In the Compatibility renderer a directional light's shadow on/off is a shader
specialization, so the first dusk recompiled every visible material: **5.1 s** on a cold shader
cache.

Keeping the shadow enabled at zero opacity instead removed the freeze but visibly changed the
night lighting (brighter and bluer), so the original behaviour was kept and the variant is
compiled at load instead.

**Change.** The load-time warm-up (`render/warmup.rs`) also collects the scene's
`DirectionalLight3D`s and flips each one's `shadow_enabled` for its second warm-up frame, so the
other shadow variant of every warmed material compiles during loading. The lights are then set
back to the values recorded when the warm-up started (not toggled back, since `PixelDayNight`
may have rewritten them in between). The cost moves to loading, plus one frame at startup with
the sun's shadow flipped.

**Files.** `addons/MarchingSquaresTerrain/rust/crates/mst/src/render/warmup.rs`,
`.../effects/day_night.rs` (comment only).

**Verified.** Cold shader cache, jumping to Night / Dawn / Noon / Night: first night 5,118 ms →
4.7–8.1 ms. A rendered night frame matches the previous build by eye.

---

## Thin weather fog as a third-person camera zooms out — `596d005` (REVERTED)

> **Reverted** in "Revert fog zoom scaling". Playtesting showed it only moved the problem: zoomed
> out the island looked right, but zoomed back in, distant objects still turned flat grey. The
> real issue is that fogged colours come out grey after the pixel pass at any zoom, not how much
> fog there is. `fog_focus_distance` no longer exists; nothing below is in the current build.

**Problem.** In rain (also Foggy, Cloudy, Storm), zooming the camera out turned the whole island
grey.

**Cause.** Godot's exponential fog grows with the distance from the **camera**:
`1 − exp(−density × d)`. Zoomed out to 100 m, even the ground under the player is 100 m away, so
with the Rain preset it is about 63 % fog, against about 12 % at the default 13 m zoom. The fog
is applied while the 3D scene is drawn, before the pixel pass; the pixel pass only snaps the
already-fogged colours to the palette.

**Change.** New `PixelWeather.fog_focus_distance` (metres, 0 = off, the default, so other
projects are unaffected). When the active camera is farther than that from its subject (the
camera's `pixel_snap_distance` meta, which the player already sets to the spring length for the
pixel renderer), the weather's fog density is scaled by `fog_focus_distance / distance`. The fog
at the subject then stays what it is at `fog_focus_distance`, while farther scenery still fades
into it. Applied where the weather publishes its mix, so both `PixelDayNight` and the weather's
own fallback grading use it. The demo sets it to 13 m (its default zoom), so the default view is
unchanged.

**Files.** `addons/MarchingSquaresTerrain/rust/crates/mst/src/effects/weather.rs`,
`demo/hex_island.tscn`.

**Verified.** Rain rendered at 13 m and 100 m zoom, setting off vs on: at 13 m the frames match;
at 100 m the island keeps its green and sand instead of turning grey, and the sea and sky show
the rain and cloud cover through less haze.

---

## Keep LOD chunks full for 3 s and keep their collision — `857a984`

**Problem.** After `9546014` the big spikes were gone, but orbiting zoomed out still dropped the
frame rate, especially with a fast mouse drag.

**Cause.** The distant LOD depends on the view (it keeps full detail between the player and the
camera), so a fast orbit keeps chunks flipping and the rebuild queue stayed full at 6 chunks per
frame. Many flips were chunks dropped to low detail the moment they left the zone and needed
again when the camera swung back. Also, a chunk's low-detail mesh became its **collision** too,
so collision far from the player changed with the camera, and rebuilding it was about a third of
the cost of a flip.

**Change.**
- **Time hysteresis.** A chunk only returns to low detail after the LOD has not needed it at
  full detail for `LOD_KEEP_SECONDS` (3 s), so swinging back and forth reuses the chunks it just
  built.
- **Collision kept when dropping detail.** A chunk that drops to low detail purely because of
  the LOD keeps its existing (finer) collision; only rebuilds for any other reason, and rises to
  full detail, rebuild collision.

**Files.** `addons/MarchingSquaresTerrain/rust/crates/mst/src/terrain/node.rs`; rebuilt
`addons/MarchingSquaresTerrain/bin/marching_squares_terrain.dll` (this build also contained
`15d3a0f` and the since-reverted `596d005`; the revert rebuilt it without the fog change).

**Verified.** Scripted orbit with real input events (mouse drag at about 360°/s, arrow keys at
120°/s), 5 s each, two runs:

| | FPS before | FPS after | Frames over 10 ms, before → after |
|---|---|---|---|
| Mouse drag, 100 m | 125 | 206–226 | 247 → 25–27 |
| Arrow keys, 100 m | 203 | 214–234 | 90 → 8–11 |
| Mouse drag, 13 m | 208 | 235–239 | 62 → 4–6 |
| Arrow keys, 13 m | 231 | 236–240 | 27 → 0 |

Cold-cache regression run of the fire / weather sequence, the night jumps and the orbits with
this build: no frame over 25 ms except one 323 ms frame after Snow in a single run, which did not
reproduce in two repeats (with this build and with the previous one).

---

## Draw fog after the palette — `d0011ab`

**Problem.** In Rain, Foggy, Cloudy and Storm, fogged ground, cliffs and paths turned flat grey,
at any zoom (worse zoomed out, where everything is far from the camera). The reverted `596d005`
only changed how much fog there was.

**Cause.** Godot's Environment fog is drawn in the 3D scene, *before* the pixel renderer snaps
the image to its palette. Fog mixes every surface towards the fog colour, so colours come out
muted; the palette's browns, tans and greens are all fairly saturated, so the nearest palette
entry to a muted brown or green is one of its greys. Shown by rendering the same Rain view with
the palette off (a hazy but clearly coloured image) and on (flat grey).

**Change.** The fog moves into the pixel renderer and is drawn after the palette:

- New `render/post_fog.rs`. While a `PixelRenderer` with the new `post_fog` setting (on by
  default, game only; the editor preview is unchanged) is running, `PixelDayNight` and
  `PixelWeather` publish their fog (colour, density, sky amount) there instead of grading the
  Environment with it.
- The Environment keeps only the sky's share: fog enabled at **zero density** with the sky
  amount. Godot treats the background as infinitely far, so this fogs the sky exactly as before,
  and rain drawn in front of the sky stays visible. Fog is never switched on or off on the
  Environment any more, so it can never trigger a specialization recompile.
- `pixel_resolve.gdshader` applies Godot's exponential fog, `1 − exp(−density × distance)`, to
  every surface after the palette, using the distance from the metadata pass. Surfaces snap to
  their own palette colour first and only then fade towards the fog colour.
- New `PixelRenderer.post_fog_dither` (off by default). Off: a smooth blend towards the fog
  colour (soft haze; fogged texels leave the palette). On: an ordered dither between the palette
  colour and the palette-snapped fog colour (strictly palette colours, but reads as a stipple).
- A scene that sets fog directly on its Environment, without a weather or day-night node, is
  taken over the same way while the renderer runs and given back when it exits.
- `PixelSky`'s cloud deck is drawn only in the colour pass, so the resolve has no distance for
  it: the deck shader (`sky_clouds.gdshader`) applies the renderer's current fog itself
  (`post_fog` uniform, fed by `PixelSky` from `post_fog::current()`).

**Files.** `addons/MarchingSquaresTerrain/rust/crates/mst/src/render/post_fog.rs` (new),
`.../render/mod.rs`, `.../render/renderer.rs`, `.../effects/day_night.rs`,
`.../effects/weather.rs`, `.../effects/sky.rs`,
`addons/MarchingSquaresTerrain/shaders/pixel_resolve.gdshader`,
`addons/MarchingSquaresTerrain/shaders/sky_clouds.gdshader`; rebuilt
`addons/MarchingSquaresTerrain/bin/marching_squares_terrain.dll`.

**Verified.**
- Rendered before / after at the default zoom for Clear, Rain and Foggy, and Rain at 100 m:
  Clear is unchanged (the only differing pixels are the waterfall's animation frame); in Rain
  and Foggy, distant cliffs, paths and trees keep their brown, tan and green and fade into the
  fog instead of turning grey; rain over the sky and the fogged cloud deck look as before.
- Dithered and smooth styles both rendered; smooth chosen as the default (the dither reads as
  noise even in light fog).
- Cold shader cache: fire / Rain / Snow / Storm sequence and the Night / Dawn / Noon jumps, worst
  frame 21 ms.

---

## Grass and trees sway with the weather's wind — `fefa2f6`

**Problem.** Playtest wish: the wind should move the grass and the trees.

**Cause.** `PixelWeather` publishes a wind every frame (`pt_wind`: direction, strength, gustiness)
and the addon ships a shared helper for it (`pixel_wind.gdshaderinc`), but neither plant shader
used it. Grass had its own wind with a fixed direction and strength, so it never reacted to the
weather. Props (trees, pines, bushes, flowers) had a `wind_sway` that was a plain sine wobble of
a few centimetres, below one pixel at the default zoom, and also ignored the weather.

**Change.**
- `mst_grass.gdshader`: new `use_global_wind` (on). The weather's wind direction replaces
  `wind_direction`, its strength (1 = the default breeze, about 3 in Rain, more in Storm)
  scales `wind_strength`, and its gustiness scales the gust fronts. At the default breeze the
  grass looks as before.
- `mst_scatter.gdshader`: `wind_sway` now means metres of lean per metre of height above the
  prop's base at wind strength 1. Each prop leans as a whole along the wind with travelling gust
  bands and a little flutter (sampled at its base so the canopy moves together), snapped to
  1/24 m steps so it stays pixel-art. The demo's existing values (tree 0.05, pine 0.03,
  bush 0.04, flowers 0.06) now give visible motion.
- Both keep their animation **speeds** fixed and only scale the **amount** with the wind. The
  shared helper's own functions speed up with the wind strength, which makes the motion jump
  while the weather blends (time × a changing rate), so they are not used for the timing.

**Files.** `addons/MarchingSquaresTerrain/shaders/mst_grass.gdshader`,
`addons/MarchingSquaresTerrain/shaders/mst_scatter.gdshader` (shaders only, no rebuild needed).

**Verified.** Rendered the demo in Clear and in Storm: the share of plant-area pixels that
change over 0.2 s doubles from 6.3 % to 12.4 %. No shader compile errors.

---

## Add god rays after the palette — `c3086f0`

**Problem.** Playtest: no visible god rays, even when changing their colour or settings.

**Cause.** The rays were computed correctly (the renderer's *God Rays* debug view shows clear
diagonal light sheets), but they were added to the colour **before** the palette snap. At the
default strength they brighten a surface only a little, and the palette rounded most of that
back to the surface's own palette colour: with the palette on they changed 38 % of pixels by
+18.6 RGB on average, against +36 with the palette off. Colour changes were rounded away too.

**Change.** New `PixelRenderer.rays_over_palette` (on by default) and the matching shader
uniform: the rays are added after the palette and the post fog (light in the air, like the
fog), in a shared `add_rays()` function. Off restores the previous order.

**Files.** `addons/MarchingSquaresTerrain/shaders/pixel_resolve.gdshader`,
`addons/MarchingSquaresTerrain/rust/crates/mst/src/render/renderer.rs`.

**Verified.** Same view, rays off vs on at the default strength: 81.6 % of pixels brightened by
+23.5 RGB on average (was 38.1 % / +18.6), and ray colour / strength changes now show. At the
default strength the rays read as a warm glow with faint diagonal banding; distinct shafts need a
higher `ray_strength` or a view towards the sun through cloud gaps (not retuned here).

---

## Keep the cloud deck above the camera — `f9989a2`

**Problem.** Found while investigating the playtest report "clouds disappear at a certain camera
height" (that report turned out to be a different bug, fixed in the next section): zoomed far
out with the camera near or above 40 m, the clouds smeared across the view or covered the island.

**Cause.** `PixelSky`'s visible cloud deck is a flat sheet at `cloud_height` (40 m) that follows
the camera. A camera near 40 m (zoomed out, tilted down) sees it **edge-on**, so it collapses to
a line. Just below that it smeared white across the whole view at a grazing angle, and above it
the clouds sat under the camera and covered the island.

**Change.** New `PixelSky.deck_clearance` (25 m by default; 0 = old behaviour): the deck is
drawn at the higher of `cloud_height` and the camera height plus the clearance, so it rises
smoothly with a climbing camera and is never edge-on or below it. Cloud shadows and god rays
still use `cloud_height`, and the deck shows the same clouds (they are sampled by XZ only).

**Files.** `addons/MarchingSquaresTerrain/rust/crates/mst/src/effects/sky.rs`; rebuilt
`addons/MarchingSquaresTerrain/bin/marching_squares_terrain.dll` (this build also contains
`c3086f0`).

**Verified.** Rendered at 100 m zoom with the camera at 29, 39 and 48 m: before, normal / white
smear over the island / clouds blanketing the island; after, a normal sky in all three.

---

## Keep the camera inside the cloud deck — "Keep the camera inside the cloud deck"

**Problem.** Playtest: zoomed in close and tilting the camera up towards the sky, the clouds
vanished at one very specific camera height.

**Cause.** `PixelSky::follow_camera` centres the deck where the view ray meets the deck,
limited only to `deck_size` away. Tilted just above the horizon (1–4° up, which with the orbit
camera is one narrow band of camera heights around 5 m), that point is hundreds of metres ahead,
so the deck was centred 500–900 m away and its near edge lay beyond the camera's 400 m far plane:
no clouds at all. Level or tilted further up, the centre was close and the clouds came back.

**Change.** The deck's horizontal shift towards the view point is capped at a quarter of
`deck_size`, so the camera always stays well inside the deck (the demo's 900 m deck then reaches
at least 675 m ahead). The clouds are sampled by world XZ, so moving the mesh never moves the
clouds themselves; nothing else changes.

**Files.** `addons/MarchingSquaresTerrain/rust/crates/mst/src/effects/sky.rs`; rebuilt
`addons/MarchingSquaresTerrain/bin/marching_squares_terrain.dll`.

**Verified.** Swept the camera tilt from 10° up to 10° down at 13 m and 8 m zoom, counting
cloud pixels in the sky: at 1–2° up they went from about 190 to about 17,000 (in line with the
neighbouring tilts); every other tilt is unchanged.

---

## Known upstream issues not changed

- `.gitmodules` still declares `addons/phantom_camera_rust` and `addons/MarchingSquaresTerrain`
  as submodules, but on `r` they are ordinary directories. Do not run
  `git submodule update --init`.
- `CREDITS.md` is referenced by the addon README but only exists on `master`.
