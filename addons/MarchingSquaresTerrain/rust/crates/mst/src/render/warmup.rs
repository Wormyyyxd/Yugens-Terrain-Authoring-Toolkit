//! First-use shader warm-up for the Compatibility renderer (driven by `PixelRenderer`).
//!
//! GL Compatibility compiles a material's shader variant the first time something draws with it,
//! stalling that frame (tens to hundreds of milliseconds per variant on a cold shader cache).
//! Effects that are hidden when the scene starts (rain, snow, splashes, fire steam, ...) or out of
//! view (a campfire across the map) therefore hitch the game the first time they appear.
//!
//! [`ShaderWarmup`] waits a few frames for the scene's effects to build their internal meshes,
//! collects every material of every `GeometryInstance3D` in the viewport (hidden and internal
//! nodes included), and draws each one for a couple of frames on a sub-pixel proxy just in front
//! of the camera: a one-instance MultiMesh with the source's instance format for MultiMesh
//! sources (the instanced variant is a separate compile), a plain quad otherwise, with the
//! source's render layers and shadow casting so every pass of the pixel pipeline (color, meta,
//! id, shadows) compiles its variant too, under a faint omni light for the omni-lit variant. The
//! compiles then land in the loading frames instead of mid-game. Proxies are freed afterwards;
//! nothing is visible (each covers far less than a pixel).
//!
//! Light and environment switches are specializations too. Directional shadows on / off (the sun
//! at dusk) are covered by flipping each directional light's shadow for the second warm-up frame
//! and restoring it afterwards. Fog on / off is kept fixed by the effects instead (see
//! `day_night::grade_fog`).

use std::collections::HashSet;

use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::light_3d::Param as LightParam;
use godot::classes::multi_mesh::TransformFormat;
use godot::classes::{Camera3D, DirectionalLight3D, GeometryInstance3D, Material, MeshInstance3D, MultiMesh, MultiMeshInstance3D, Node, Node3D, OmniLight3D, QuadMesh};
use godot::prelude::*;

use crate::util::*;

/// Frames to wait before collecting, so effects have built their internal meshes.
const SETTLE_FRAMES: u32 = 3;
/// Frames the proxies are drawn.
const DRAW_FRAMES: u32 = 2;
/// Give up waiting for a camera after this many frames.
const MAX_WAIT_FRAMES: u32 = 120;
/// Proxy size (m): a draw call is issued (so the variant compiles) but no pixel is covered.
const PROXY_SCALE: f32 = 1e-4;

/// How a proxy must be drawn for its source's shader variants to match.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Variant {
    material: i64,
    /// `Some((use_custom_data, use_colors))` for MultiMesh sources.
    instanced: Option<(bool, bool)>,
    layers: u32,
    shadows: bool,
}

#[derive(Default)]
pub struct ShaderWarmup {
    frame: u32,
    done: bool,
    root: Option<Gd<Node3D>>,
    /// Proxies drawn by the last run (for tests / diagnostics).
    count: usize,
    /// Directional lights of the scene and their own `shadow_enabled`: flipped for one warm-up
    /// frame (shadow on / off is a specialization too, and PixelDayNight switches the sun's
    /// shadow off at dusk), then restored to these values.
    lights: Vec<(Gd<DirectionalLight3D>, bool)>,
}

impl ShaderWarmup {
    pub fn is_done(&self) -> bool {
        self.done
    }

    pub fn proxy_count(&self) -> usize {
        self.count
    }

    /// Advances the warm-up by one frame. `owner` is the renderer (proxies live under it as an
    /// internal child); `scan` is the subtree whose materials are warmed (the game viewport).
    pub fn step(&mut self, owner: &mut Gd<Node>, scan: Gd<Node>, cam: Option<Gd<Camera3D>>) {
        if self.done {
            return;
        }
        self.frame += 1;
        if self.root.is_none() {
            if self.frame < SETTLE_FRAMES {
                return;
            }
            let Some(cam) = cam else {
                if self.frame > MAX_WAIT_FRAMES {
                    self.done = true;
                }
                return;
            };
            self.spawn(owner, &scan, &cam);
            self.frame = 0;
            return;
        }
        if self.frame >= DRAW_FRAMES {
            self.clear();
            self.done = true;
        } else if self.frame == 1 {
            // Second warm-up frame: the other directional-shadow variant.
            for (l, own) in &mut self.lights {
                if l.is_instance_valid() {
                    l.set_shadow(!*own);
                }
            }
        }
    }

    /// Frees the proxies and restores the lights (also when the renderer leaves the tree
    /// mid-warm-up).
    pub fn clear(&mut self) {
        free_node(self.root.take());
        for (mut l, own) in self.lights.drain(..) {
            if l.is_instance_valid() {
                l.set_shadow(own);
            }
        }
    }

    fn spawn(&mut self, owner: &mut Gd<Node>, scan: &Gd<Node>, cam: &Gd<Camera3D>) {
        let mut found: Vec<(Variant, Gd<Material>)> = Vec::new();
        let mut seen: HashSet<Variant> = HashSet::new();
        let mut lights: Vec<Gd<DirectionalLight3D>> = Vec::new();
        collect(scan, owner, &mut seen, &mut found, &mut lights);
        self.lights = lights.into_iter().map(|l| {
            let own = l.has_shadow();
            (l, own)
        }).collect();
        self.count = found.len();
        if found.is_empty() {
            return;
        }
        // Just beyond the near plane, on the view axis: inside every pass camera's frustum
        // (they copy the game camera).
        let xf = cam.get_global_transform();
        let ahead = (cam.get_near() + 0.5).max(0.1);
        let mut root = Node3D::new_alloc();
        root.set_name("ShaderWarmup");
        root.set_as_top_level(true);
        root.add_to_group(crate::debug::diagnostics::PICK_IGNORE_GROUP);
        add_internal(owner, &root);
        crate::writes::set_global_transform(&mut root, Transform3D::new(Basis::IDENTITY, xf.origin - xf.basis.col_c() * ahead));
        // A faint omni light over the proxies: materials compile their omni-lit variant too, so
        // the first lantern / fire light that reaches them does not hitch (fire lights are
        // created mid-game). Range just covers the proxies; energy is far below one colour step.
        let mut omni = OmniLight3D::new_alloc();
        omni.set_param(LightParam::RANGE, 0.05);
        omni.set_param(LightParam::ENERGY, 0.001);
        root.add_child(&omni);
        let mut quad = QuadMesh::new_gd();
        quad.set_size(Vector2::new(1.0, 1.0));
        let small = Transform3D::new(Basis::from_scale(Vector3::splat(PROXY_SCALE)), Vector3::ZERO);
        for (v, mat) in found {
            let mut g: Gd<GeometryInstance3D> = match v.instanced {
                Some((custom, colors)) => {
                    let mut mm = MultiMesh::new_gd();
                    mm.set_transform_format(TransformFormat::TRANSFORM_3D);
                    mm.set_use_custom_data(custom);
                    mm.set_use_colors(colors);
                    mm.set_mesh(&quad);
                    mm.set_instance_count(1);
                    mm.set_instance_transform(0, small);
                    let e = Vector3::splat(PROXY_SCALE);
                    mm.set_custom_aabb(Aabb::new(-e, e * 2.0));
                    let mut mi = MultiMeshInstance3D::new_alloc();
                    mi.set_multimesh(&mm);
                    mi.upcast()
                }
                None => {
                    let mut mi = MeshInstance3D::new_alloc();
                    mi.set_mesh(&quad);
                    mi.set_scale(Vector3::splat(PROXY_SCALE));
                    mi.upcast()
                }
            };
            g.set_material_override(&mat);
            g.set_layer_mask(v.layers);
            g.set_cast_shadows_setting(if v.shadows { ShadowCastingSetting::ON } else { ShadowCastingSetting::OFF });
            g.set_extra_cull_margin(1.0);
            root.add_child(&g);
        }
        self.root = Some(root);
    }
}

/// Collects one entry per distinct (material, draw variant) under `node`, internal children
/// included; skips `skip` (the renderer's own pipeline).
fn collect(node: &Gd<Node>, skip: &Gd<Node>, seen: &mut HashSet<Variant>, out: &mut Vec<(Variant, Gd<Material>)>, lights: &mut Vec<Gd<DirectionalLight3D>>) {
    if node == skip {
        return;
    }
    if let Ok(l) = node.clone().try_cast::<DirectionalLight3D>() {
        lights.push(l);
    }
    if let Ok(g) = node.clone().try_cast::<GeometryInstance3D>() {
        let layers = g.get_layer_mask();
        let shadows = g.get_cast_shadows_setting() != ShadowCastingSetting::OFF;
        let mut mats: Vec<Gd<Material>> = Vec::new();
        let mut instanced = None;
        let ovr = g.get_material_override();
        if let Some(m) = &ovr {
            mats.push(m.clone());
        }
        if let Some(m) = g.get_material_overlay() {
            mats.push(m);
        }
        if let Ok(mi) = g.clone().try_cast::<MeshInstance3D>() {
            if ovr.is_none()
                && let Some(mesh) = mi.get_mesh()
            {
                for s in 0..mesh.get_surface_count() {
                    if let Some(m) = mi.get_surface_override_material(s).or_else(|| mesh.surface_get_material(s)) {
                        mats.push(m);
                    }
                }
            }
        } else if let Ok(mmi) = g.clone().try_cast::<MultiMeshInstance3D>() {
            let mm = mmi.get_multimesh();
            if let Some(mm) = &mm {
                instanced = Some((mm.is_using_custom_data(), mm.is_using_colors()));
            }
            if ovr.is_none()
                && let Some(mesh) = mm.and_then(|mm| mm.get_mesh())
            {
                for s in 0..mesh.get_surface_count() {
                    if let Some(m) = mesh.surface_get_material(s) {
                        mats.push(m);
                    }
                }
            }
        }
        for m in mats {
            let v = Variant { material: m.instance_id().to_i64(), instanced, layers, shadows };
            if seen.insert(v) {
                out.push((v, m));
            }
        }
    }
    let n = node.get_child_count_ex().include_internal(true).done();
    for i in 0..n {
        if let Some(c) = node.get_child_ex(i).include_internal(true).done() {
            collect(&c, skip, seen, out, lights);
        }
    }
}
