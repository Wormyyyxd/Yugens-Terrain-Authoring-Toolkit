// Marching Squares Terrain - scatter props (rocks, bushes, trees, custom meshes) drawn by the
// terrain's per-tile MultiMeshes. INSTANCE_CUSTOM = (hue shift in turns, value offset, 0, 0).
// Toon-lit like the pixel object shader; joins the pixel pipeline (outlines, pixel size).
shader_type spatial;
render_mode specular_disabled, cull_back;

#define PT_FLAGS_MATERIAL
#include "res://addons/MarchingSquaresTerrain/shaders/pixel_common.gdshaderinc"
// Fog of war on non-terrain surfaces (pt_fog_surface).
#include "res://addons/MarchingSquaresTerrain/shaders/mst_cells.gdshaderinc"
// [PixelWeather snow] snow caps on upward faces (pixel_weather.gdshaderinc)
#include "res://addons/MarchingSquaresTerrain/shaders/pixel_weather.gdshaderinc"
// [PixelWeather wind] pt_wind (pixel_wind.gdshaderinc)
#include "res://addons/MarchingSquaresTerrain/shaders/pixel_wind.gdshaderinc"

uniform vec4 albedo_color : source_color = vec4(1.0);
uniform sampler2D albedo_texture : source_color, filter_nearest_mipmap, repeat_enable;
uniform bool use_albedo_texture = false;
uniform bool use_vertex_color = true;
uniform bool use_alpha_scissor = false;
uniform float alpha_scissor_threshold : hint_range(0.0, 1.0) = 0.5;
uniform int bands : hint_range(1, 8) = 3;
uniform float shade_floor : hint_range(0.0, 1.0) = 0.35;
/// Lean in the wind (0 = static): metres of sway per metre of height above the prop's base at
/// wind strength 1 (PixelWeather's pt_wind: the default breeze; storms 3-4). The whole prop
/// leans together (gusts and flutter are sampled at its base), in 1/24 m steps.
uniform float wind_sway = 0.0;
/// Flutter rate (fixed, so weather changes never make the animation jump).
uniform float wind_speed = 0.8;

varying vec3 world_pos;
varying float cloud;
varying vec2 jitter;
varying flat float lod_keep;

vec3 pt_hue_shift(vec3 c, float turns) {
	// Rotation around the grey axis (YIQ-free Rodrigues form).
	float a = turns * 6.2831853;
	vec3 k = vec3(0.57735);
	float ca = cos(a);
	return c * ca + cross(k, c) * sin(a) + k * dot(k, c) * (1.0 - ca);
}

void vertex() {
	jitter = INSTANCE_CUSTOM.rg;
	if (wind_sway > 0.0) {
		vec3 o = MODEL_MATRIX[3].xyz;
		vec2 dir = length(pt_wind.xy) > 1e-3 ? normalize(pt_wind.xy) : vec2(1.0, 0.0);
		// Gust bands rolling along the wind at a fixed pace (cf. pt_wind_gust, whose pace follows
		// the strength and so jumps while the weather blends), strengthened by gustiness.
		float along = dot(o.xz, dir) - pt_time * 3.0;
		float across = dot(o.xz, vec2(-dir.y, dir.x));
		float g = pt_vnoise(vec2(along * 0.08, across * 0.05)) * 0.7 + pt_vnoise(vec2(along * 0.21, across * 0.13 + 4.0)) * 0.3;
		float gust = smoothstep(0.35, 1.0, g) * (0.5 + clamp(pt_wind.w, 0.0, 1.0));
		float flutter = sin(pt_time * wind_speed * 2.5 + dot(o.xz, vec2(0.13, 0.17)) * 6.0);
		float h = max(VERTEX.y, 0.0) * length(MODEL_MATRIX[1].xyz);
		float bend = wind_sway * clamp(pt_wind.z, 0.0, 6.0) * h * (0.35 + gust + flutter * (0.15 + 0.25 * gust));
		bend = floor(bend * 24.0 + 0.5) / 24.0;
		VERTEX += inverse(mat3(MODEL_MATRIX)) * vec3(dir.x * bend, -abs(bend) * 0.15, dir.y * bend);
	}
	world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
	lod_keep = pt_lod_keep(MODEL_MATRIX[3].xyz);
}

void fragment() {
	if (lod_keep < 1.0 && lod_keep <= pt_bayer4(FRAGCOORD.xy)) {
		discard; // distant LOD fade (whole prop, by its origin)
	}
	vec4 c = albedo_color;
	if (use_albedo_texture) {
		c *= texture(albedo_texture, UV);
	}
	if (use_vertex_color) {
		c *= COLOR;
	}
	if (use_alpha_scissor) {
		ALPHA = c.a;
		ALPHA_SCISSOR_THRESHOLD = alpha_scissor_threshold;
	}
	vec3 rgb = c.rgb;
	if (jitter.x != 0.0) {
		rgb = max(pt_hue_shift(rgb, jitter.x), vec3(0.0));
	}
	ALBEDO = clamp(rgb * (1.0 + jitter.y), 0.0, 1.0);
	ALBEDO = pt_snow_cap(ALBEDO, world_pos, normalize((INV_VIEW_MATRIX * vec4(NORMAL, 0.0)).xyz)); // [PixelWeather snow]
	cloud = pt_cloud_shadow(world_pos);
	{ vec4 pt_fs = pt_fog_surface(ALBEDO, world_pos, mst_pixel_lattice(world_pos, INV_VIEW_MATRIX, FRAGCOORD.xy)); ALBEDO = pt_fs.rgb; EMISSION *= pt_fs.a; } // [fog of war]
	PT_META_OUTPUT(NORMAL)
}

void light() {
	PT_TOON_LIGHT(bands, shade_floor, cloud)
}
