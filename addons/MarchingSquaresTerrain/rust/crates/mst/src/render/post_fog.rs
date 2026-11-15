//! Post-palette fog: the hand-off between the fog producers (PixelDayNight, PixelWeather) and
//! `PixelRenderer`, which draws the fog in its resolve pass after the palette instead of letting
//! the 3D scene draw it.
//!
//! Why: Environment fog mixes every surface towards the fog colour before the resolve snaps the
//! image to the palette. Fogged colours are muted, while palette ramps are mostly saturated, so a
//! hazy brown, tan or green lands nearest the palette's grey ramp: distant ground in rain read as
//! flat grey. With post fog the colour pass renders without fog, every surface snaps to its own
//! palette colour, and the resolve then fades it towards the (palette-snapped) fog colour by
//! distance, as an ordered dither when the palette is on.
//!
//! While a renderer with `post_fog` is in the tree ([`active`]), producers [`publish`] their fog
//! every frame and leave the Environment only the sky's share ([`grade_sky`]: fog enabled at zero
//! density). Fog on / off is then never toggled on the Environment, which in the Compatibility
//! renderer would recompile every visible material (a shader specialization).

use std::cell::Cell;
use std::sync::atomic::{AtomicI32, Ordering};

use godot::prelude::*;

/// Fog for the resolve pass, in Godot's exponential fog terms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fog {
    /// Fog colour (sRGB, like `Environment.fog_light_color` x `fog_light_energy`).
    pub color: Color,
    /// `1 - exp(-density * distance)` of the surface colour is replaced by the fog colour.
    pub density: f32,
    /// Fog amount on the background (it counts as infinitely far), like `fog_sky_affect`.
    pub sky_affect: f32,
}

static RENDERERS: AtomicI32 = AtomicI32::new(0);

thread_local! {
    static PUBLISHED: Cell<Option<Fog>> = const { Cell::new(None) };
    /// The fog the renderer drew last frame, for colour-only geometry that fogs itself.
    static CURRENT: Cell<Option<Fog>> = const { Cell::new(None) };
}

/// The fog the renderer drew last frame (None without post fog or fog). Colour-only geometry
/// that is far away (PixelSky's cloud deck) applies it in its own shader: the resolve has no
/// distance for it, and the Environment's fog density is held at zero.
pub fn current() -> Option<Fog> {
    CURRENT.with(|c| c.get())
}

pub(crate) fn set_current(fog: Option<Fog>) {
    CURRENT.with(|c| c.set(fog));
}

/// Whether a renderer draws the fog (producers then publish instead of grading the Environment).
pub fn active() -> bool {
    RENDERERS.load(Ordering::Relaxed) > 0
}

/// Registers / unregisters a renderer with post fog on (balanced calls).
pub(crate) fn register(on: bool) {
    if on {
        RENDERERS.fetch_add(1, Ordering::Relaxed);
    } else {
        RENDERERS.fetch_sub(1, Ordering::Relaxed);
    }
}

/// This frame's fog from a producer (the last call in a frame wins).
pub fn publish(fog: Fog) {
    PUBLISHED.with(|p| p.set(Some(fog)));
}

/// The fog published since the last call (the renderer reads it once per frame, after the
/// producers' `process`).
pub(crate) fn take() -> Option<Fog> {
    PUBLISHED.with(|p| p.take())
}

/// Leaves the Environment only the fog on the background: fog stays enabled (fog on / off is a
/// specialization, see the module docs) at zero density, so surfaces get none from it, while the
/// sky, which counts as infinitely far, still takes `sky_affect` of the fog colour; colour-only
/// effects drawn over the sky (rain) stay unfogged, as with Godot's fog. `None` = no fog.
pub fn grade_sky(env: &mut Gd<godot::classes::Environment>, fog: Option<Fog>) {
    let (color, sky) = match fog {
        Some(f) if f.density > 1e-5 => (f.color, f.sky_affect),
        _ => (env.get_fog_light_color(), 0.0),
    };
    crate::set_changed!(env, is_fog_enabled, set_fog_enabled, true);
    crate::set_changed!(env, get_fog_density, set_fog_density, 0.0);
    crate::set_changed!(env, get_fog_sky_affect, set_fog_sky_affect, sky);
    crate::set_changed!(env, get_fog_light_color, set_fog_light_color, color);
}

/// Reads the fog an Environment would draw (None when its fog is off).
pub(crate) fn from_environment(env: &Gd<godot::classes::Environment>) -> Option<Fog> {
    if !env.is_fog_enabled() {
        return None;
    }
    let c = env.get_fog_light_color();
    let e = env.get_fog_light_energy();
    Some(Fog { color: Color::from_rgb(c.r * e, c.g * e, c.b * e), density: env.get_fog_density(), sky_affect: env.get_fog_sky_affect() })
}
