//! Per-pixel kernels, each defined twice: in WGSL for the GPU backend and in
//! Rust for the CPU fallback. Both versions share the same parameter layout
//! (up to 32 floats, `P(i)` / `env.p[i]`) and the same conventions:
//!
//! * `p` is the output pixel center (`i + 0.5`, `j + 0.5`).
//! * `node(p)` maps it to node space; `extent` is the node's logical size.
//! * `sN(p)` samples input `N` through its texel map and sampling mode.
//!
//! Adding a kernel means adding a [`Kernel`] constant here; the GPU backend
//! compiles it lazily on first use.

use super::tile::{clamp_premul, premul, unpremul, CpuTile, Px, SampleMode, CLEAR};
use rayon::prelude::*;

pub const MAX_INPUTS: usize = 3;
pub const MAX_PARAMS: usize = 32;

pub struct Kernel {
    pub name: &'static str,
    pub inputs: usize,
    /// WGSL source defining `fn kernel(p: vec2<f32>) -> vec4<f32>`.
    pub wgsl: &'static str,
    /// CPU version, called once per output row.
    pub cpu: fn(&Env, Row),
}

/// One output row handed to a CPU kernel. Inputs arrive already sampled at
/// the row's pixel centers, so kernels index them like the output and the
/// per-pixel code inlines into a tight loop.
pub struct Row<'a> {
    /// Pixel-center y of the row.
    pub y: f32,
    /// Input `N` at each pixel of the row (transparent when absent).
    pub s: [&'a [Px]; MAX_INPUTS],
    pub out: &'a mut [Px],
}

impl Row<'_> {
    /// Sets each pixel to `f(i, x, y)`, given its index and pixel center.
    #[inline(always)]
    pub fn fill(self, f: impl Fn(usize, f32, f32) -> Px) {
        let y = self.y;
        for (i, px) in self.out.iter_mut().enumerate() {
            *px = f(i, i as f32 + 0.5, y);
        }
    }
}

pub struct EnvInput<'a> {
    pub tile: Option<&'a CpuTile>,
    pub map: [f32; 4],
    pub mode: SampleMode,
}

pub struct Env<'a> {
    pub inputs: Vec<EnvInput<'a>>,
    pub node_map: [f32; 4],
    pub extent: [f32; 2],
    pub p: [f32; MAX_PARAMS],
}

impl EnvInput<'_> {
    /// Row `j` of the tile as-is, when this input is sampled exactly on the
    /// output grid (nearest, identity map) and covers the whole row.
    fn aligned_row(&self, j: usize, w: usize) -> Option<&[Px]> {
        let t = self.tile?;
        let aligned = self.mode == SampleMode::Nearest && self.map == [1.0, 0.0, 1.0, 0.0];
        (aligned && t.w as usize >= w && j < t.h as usize).then(|| &t.data[j * t.w as usize..][..w])
    }

    /// Samples the row with pixel-center y `py` into `out`.
    fn sample_row(&self, tile: &CpuTile, py: f32, out: &mut [Px]) {
        let m = self.map;
        let v = m[2] * py + m[3];
        #[inline(always)]
        fn row(out: &mut [Px], m: [f32; 4], f: impl Fn(f32) -> Px) {
            for (i, px) in out.iter_mut().enumerate() {
                *px = f(m[0] * (i as f32 + 0.5) + m[1]);
            }
        }
        // One loop per mode, so the mode match is hoisted out of the loop.
        match self.mode {
            SampleMode::Nearest => row(out, m, |u| tile.sample(SampleMode::Nearest, u, v)),
            SampleMode::Bilinear => row(out, m, |u| tile.sample(SampleMode::Bilinear, u, v)),
            SampleMode::Bicubic => row(out, m, |u| tile.sample(SampleMode::Bicubic, u, v)),
        }
    }
}

impl Env<'_> {
    #[inline]
    pub fn node(&self, px: f32, py: f32) -> (f32, f32) {
        let m = self.node_map;
        (m[0] * px + m[1], m[2] * py + m[3])
    }

    #[inline]
    pub fn color(&self, at: usize) -> Px {
        premul([self.p[at], self.p[at + 1], self.p[at + 2]], self.p[at + 3])
    }
}

/// Shared WGSL prelude. `{SAMPLERS}` is replaced by one sampling function per
/// input binding.
pub const WGSL_PRELUDE: &str = r#"
struct Uniforms {
    out_size: vec4<u32>,
    node_map: vec4<f32>,
    extent: vec4<f32>,
    in_map: array<vec4<f32>, 3>,
    in_mode: vec4<u32>,
    params: array<vec4<f32>, 8>,
};

@group(0) @binding(0) var<uniform> U: Uniforms;
@group(0) @binding(1) var in0: texture_2d<f32>;
@group(0) @binding(2) var in1: texture_2d<f32>;
@group(0) @binding(3) var in2: texture_2d<f32>;
@group(0) @binding(4) var out_tex: texture_storage_2d<rgba32float, write>;

fn P(i: u32) -> f32 { return U.params[i / 4u][i % 4u]; }
fn color_param(i: u32) -> vec4<f32> {
    let c = vec4<f32>(P(i), P(i + 1u), P(i + 2u), P(i + 3u));
    return vec4<f32>(c.rgb * c.a, c.a);
}
fn node(p: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(U.node_map.x * p.x + U.node_map.y, U.node_map.z * p.y + U.node_map.w);
}
fn extent() -> vec2<f32> { return U.extent.xy; }
fn unpremul(c: vec4<f32>) -> vec3<f32> {
    if (c.a <= 1e-6) { return vec3<f32>(0.0); }
    return c.rgb / c.a;
}
fn clamp_premul(c: vec4<f32>) -> vec4<f32> {
    let a = clamp(c.a, 0.0, 1.0);
    return vec4<f32>(clamp(c.rgb, vec3<f32>(0.0), vec3<f32>(a)), a);
}
fn catmull_rom(t: f32) -> vec4<f32> {
    let t2 = t * t;
    let t3 = t2 * t;
    return 0.5 * vec4<f32>(-t3 + 2.0 * t2 - t, 3.0 * t3 - 5.0 * t2 + 2.0, -3.0 * t3 + 4.0 * t2 + t, t3 - t2);
}
{SAMPLERS}
"#;

pub const WGSL_SAMPLER_TEMPLATE: &str = r#"
fn fetchN(x: i32, y: i32) -> vec4<f32> {
    let d = vec2<i32>(textureDimensions(inN));
    return textureLoad(inN, clamp(vec2<i32>(x, y), vec2<i32>(0), d - 1), 0);
}
fn sN(p: vec2<f32>) -> vec4<f32> {
    let mode = U.in_mode[N];
    if (mode == 3u) { return vec4<f32>(0.0); }
    let m = U.in_map[N];
    let u = vec2<f32>(m.x * p.x + m.y, m.z * p.y + m.w);
    let d = vec2<f32>(textureDimensions(inN));
    if (!(u.x >= 0.0 && u.y >= 0.0 && u.x < d.x && u.y < d.y)) { return vec4<f32>(0.0); }
    if (mode == 0u) {
        return fetchN(i32(u.x), i32(u.y));
    }
    let q = u - 0.5;
    let q0 = floor(q);
    let f = q - q0;
    let i0 = vec2<i32>(q0);
    if (mode == 1u) {
        let a = fetchN(i0.x, i0.y);
        let b = fetchN(i0.x + 1, i0.y);
        let c = fetchN(i0.x, i0.y + 1);
        let e = fetchN(i0.x + 1, i0.y + 1);
        return mix(mix(a, b, f.x), mix(c, e, f.x), f.y);
    }
    let wx = catmull_rom(f.x);
    let wy = catmull_rom(f.y);
    var acc = vec4<f32>(0.0);
    for (var j = 0; j < 4; j++) {
        for (var i = 0; i < 4; i++) {
            acc += fetchN(i0.x - 1 + i, i0.y - 1 + j) * wx[i] * wy[j];
        }
    }
    return clamp_premul(acc);
}
"#;

pub const WGSL_MAIN: &str = r#"
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= U.out_size.x || gid.y >= U.out_size.y) { return; }
    let p = vec2<f32>(f32(gid.x) + 0.5, f32(gid.y) + 0.5);
    textureStore(out_tex, vec2<i32>(gid.xy), kernel(p));
}
"#;

pub fn wgsl_source(k: &Kernel) -> String {
    let samplers: String = (0..MAX_INPUTS)
        .map(|i| WGSL_SAMPLER_TEMPLATE.replace("inN", &format!("in{i}")).replace("fetchN", &format!("fetch{i}")).replace("sN", &format!("s{i}")).replace("[N]", &format!("[{i}]")))
        .collect();
    let mut s = WGSL_PRELUDE.replace("{SAMPLERS}", &samplers);
    s.push_str(WGSL_NOISE);
    s.push_str(k.wgsl);
    s.push_str(WGSL_MAIN);
    s
}

// ---------------------------------------------------------------------------
// Basic kernels
// ---------------------------------------------------------------------------

pub const RESAMPLE: Kernel = Kernel {
    name: "resample",
    inputs: 1,
    wgsl: "fn kernel(p: vec2<f32>) -> vec4<f32> { return s0(p); }",
    cpu: |_, r| r.out.copy_from_slice(r.s[0]),
};

pub const INVERT: Kernel = Kernel {
    name: "invert",
    inputs: 1,
    wgsl: "fn kernel(p: vec2<f32>) -> vec4<f32> { let c = s0(p); return vec4<f32>(vec3<f32>(c.a) - c.rgb, c.a); }",
    cpu: |_, r| {
        let [s0, ..] = r.s;
        r.fill(|i, _, _| {
            let c = s0[i];
            [c[3] - c[0], c[3] - c[1], c[3] - c[2], c[3]]
        })
    },
};

/// Porter-Duff "over": input 0 in front of input 1.
pub const OVER: Kernel = Kernel {
    name: "over",
    inputs: 2,
    wgsl: "fn kernel(p: vec2<f32>) -> vec4<f32> { let f = s0(p); let b = s1(p); return f + b * (1.0 - f.a); }",
    cpu: |_, r| {
        let [s0, s1, ..] = r.s;
        r.fill(|i, _, _| {
            let f = s0[i];
            let b = s1[i];
            let k = 1.0 - f[3];
            [f[0] + b[0] * k, f[1] + b[1] * k, f[2] + b[2] * k, f[3] + b[3] * k]
        })
    },
};

/// Multiplies input 0 by the mask in input 1's red channel.
/// P0: invert mask (keep background instead of foreground).
pub const MASK: Kernel = Kernel {
    name: "mask",
    inputs: 2,
    wgsl: r#"fn kernel(p: vec2<f32>) -> vec4<f32> {
        var m = clamp(s1(p).r, 0.0, 1.0);
        if (P(0u) > 0.5) { m = 1.0 - m; }
        return s0(p) * m;
    }"#,
    cpu: |e, r| {
        let [s0, s1, ..] = r.s;
        r.fill(|i, _, _| {
            let mut m = s1[i][0].clamp(0.0, 1.0);
            if e.p[0] > 0.5 {
                m = 1.0 - m;
            }
            let c = s0[i];
            [c[0] * m, c[1] * m, c[2] * m, c[3] * m]
        })
    },
};

/// Keeps input 0's pixels whose centre is inside (P4 = 0) or outside
/// (P4 = 1) the node-space rectangle [P0, P2) × [P1, P3); the rest become
/// transparent. Deciding by pixel centre gives every pixel to exactly one
/// side, so the two parts of a split add up to the original.
pub const SPLIT: Kernel = Kernel {
    name: "split",
    inputs: 1,
    wgsl: r#"fn kernel(p: vec2<f32>) -> vec4<f32> {
        let n = node(p);
        let inside = n.x >= P(0u) && n.x < P(2u) && n.y >= P(1u) && n.y < P(3u);
        if (inside == (P(4u) < 0.5)) { return s0(p); }
        return vec4<f32>(0.0);
    }"#,
    cpu: |e, r| {
        let [s0, ..] = r.s;
        r.fill(|i, x, y| {
            let (nx, ny) = e.node(x, y);
            let inside = nx >= e.p[0] && nx < e.p[2] && ny >= e.p[1] && ny < e.p[3];
            if inside == (e.p[4] < 0.5) { s0[i] } else { [0.0; 4] }
        })
    },
};

/// Reflection across an axis. Input 0 is the direct image, input 1 the same
/// image sampled through the mirror transform.
/// P0 orientation (0 = vertical axis, 1 = horizontal axis), P1 axis position
/// in node units, P2 side (0 = low side copied onto high side, 1 = reverse),
/// P3 mode (0 replace, 1 additive, 2 subtractive).
pub const REFLECT: Kernel = Kernel {
    name: "reflect",
    inputs: 2,
    wgsl: r#"fn kernel(p: vec2<f32>) -> vec4<f32> {
        let n = node(p);
        let d = s0(p);
        var coord = n.x;
        var lim = extent().x;
        if (P(0u) > 0.5) { coord = n.y; lim = extent().y; }
        let axis = P(1u);
        var dest = coord > axis;
        if (P(2u) > 0.5) { dest = coord < axis; }
        let mc = 2.0 * axis - coord;
        if (!dest || mc < 0.0 || mc > lim) { return d; }
        let m = s1(p);
        let mode = u32(P(3u) + 0.5);
        if (mode == 0u) { return m; }
        if (mode == 1u) { return clamp_premul(d + m); }
        return vec4<f32>(max(d.rgb - m.rgb, vec3<f32>(0.0)), d.a);
    }"#,
    cpu: |e, r| {
        let [s0, s1, ..] = r.s;
        r.fill(|i, x, y| {
            let (nx, ny) = e.node(x, y);
            let d = s0[i];
            let (coord, lim) = if e.p[0] > 0.5 { (ny, e.extent[1]) } else { (nx, e.extent[0]) };
            let axis = e.p[1];
            let dest = if e.p[2] > 0.5 { coord < axis } else { coord > axis };
            let mc = 2.0 * axis - coord;
            if !dest || mc < 0.0 || mc > lim {
                return d;
            }
            let m = s1[i];
            match (e.p[3] + 0.5) as u32 {
                0 => m,
                1 => clamp_premul([d[0] + m[0], d[1] + m[1], d[2] + m[2], d[3] + m[3]]),
                _ => [(d[0] - m[0]).max(0.0), (d[1] - m[1]).max(0.0), (d[2] - m[2]).max(0.0), d[3]],
            }
        })
    },
};

// ---------------------------------------------------------------------------
// Blend modes (https://en.wikipedia.org/wiki/Blend_modes, W3C compositing)
// ---------------------------------------------------------------------------

pub const BLEND_MODES: &[(&str, &str)] = &[
    ("normal", "Normal"),
    ("dissolve", "Dissolve"),
    ("multiply", "Multiply"),
    ("screen", "Screen"),
    ("overlay", "Overlay"),
    ("hard_light", "Hard light"),
    ("soft_light", "Soft light"),
    ("color_dodge", "Color dodge"),
    ("linear_dodge", "Linear dodge (add)"),
    ("color_burn", "Color burn"),
    ("linear_burn", "Linear burn"),
    ("vivid_light", "Vivid light"),
    ("linear_light", "Linear light"),
    ("pin_light", "Pin light"),
    ("hard_mix", "Hard mix"),
    ("divide", "Divide"),
    ("subtract", "Subtract"),
    ("difference", "Difference"),
    ("exclusion", "Exclusion"),
    ("darken", "Darken only"),
    ("lighten", "Lighten only"),
    ("hue", "Hue"),
    ("saturation", "Saturation"),
    ("color", "Color"),
    ("luminosity", "Luminosity"),
    ("and", "Logic AND"),
    ("or", "Logic OR"),
    ("xor", "Logic XOR"),
];

#[inline]
fn screen(a: f32, b: f32) -> f32 {
    a + b - a * b
}
#[inline]
fn color_dodge(cb: f32, cs: f32) -> f32 {
    if cb <= 0.0 { 0.0 } else if cs >= 1.0 { 1.0 } else { (cb / (1.0 - cs)).min(1.0) }
}
#[inline]
fn color_burn(cb: f32, cs: f32) -> f32 {
    if cb >= 1.0 { 1.0 } else if cs <= 0.0 { 0.0 } else { 1.0 - ((1.0 - cb) / cs).min(1.0) }
}
#[inline]
fn hard_light(cb: f32, cs: f32) -> f32 {
    if cs <= 0.5 { cb * 2.0 * cs } else { screen(cb, 2.0 * cs - 1.0) }
}
#[inline]
fn soft_light(cb: f32, cs: f32) -> f32 {
    if cs <= 0.5 {
        cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb)
    } else {
        let d = if cb <= 0.25 { ((16.0 * cb - 12.0) * cb + 4.0) * cb } else { cb.sqrt() };
        cb + (2.0 * cs - 1.0) * (d - cb)
    }
}
#[inline]
fn vivid_light(cb: f32, cs: f32) -> f32 {
    if cs <= 0.5 { color_burn(cb, 2.0 * cs) } else { color_dodge(cb, 2.0 * cs - 1.0) }
}
#[inline]
fn logic(cb: f32, cs: f32, op: u32) -> f32 {
    let a = (cb.clamp(0.0, 1.0) * 255.0).round() as u32;
    let b = (cs.clamp(0.0, 1.0) * 255.0).round() as u32;
    let r = match op {
        0 => a & b,
        1 => a | b,
        _ => a ^ b,
    };
    r as f32 / 255.0
}

fn blend_channel(mode: u32, cb: f32, cs: f32) -> f32 {
    match mode {
        2 => cb * cs,
        3 => screen(cb, cs),
        4 => hard_light(cs, cb),
        5 => hard_light(cb, cs),
        6 => soft_light(cb, cs),
        7 => color_dodge(cb, cs),
        8 => (cb + cs).min(1.0),
        9 => color_burn(cb, cs),
        10 => (cb + cs - 1.0).max(0.0),
        11 => vivid_light(cb, cs),
        12 => (cb + 2.0 * cs - 1.0).clamp(0.0, 1.0),
        13 => {
            if cs <= 0.5 { cb.min(2.0 * cs) } else { cb.max(2.0 * cs - 1.0) }
        }
        14 => {
            if vivid_light(cb, cs) >= 0.5 { 1.0 } else { 0.0 }
        }
        15 => {
            if cs <= 0.0 { 1.0 } else { (cb / cs).clamp(0.0, 1.0) }
        }
        16 => (cb - cs).max(0.0),
        17 => (cb - cs).abs(),
        18 => cb + cs - 2.0 * cb * cs,
        19 => cb.min(cs),
        20 => cb.max(cs),
        25 => logic(cb, cs, 0),
        26 => logic(cb, cs, 1),
        27 => logic(cb, cs, 2),
        _ => cs,
    }
}

#[inline]
fn lum(c: [f32; 3]) -> f32 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}
fn clip_color(c: [f32; 3]) -> [f32; 3] {
    let l = lum(c);
    let n = c[0].min(c[1]).min(c[2]);
    let x = c[0].max(c[1]).max(c[2]);
    let mut o = c;
    if n < 0.0 {
        for v in o.iter_mut() {
            *v = l + (*v - l) * l / (l - n).max(1e-6);
        }
    }
    if x > 1.0 {
        for v in o.iter_mut() {
            *v = l + (*v - l) * (1.0 - l) / (x - l).max(1e-6);
        }
    }
    o
}
fn set_lum(c: [f32; 3], l: f32) -> [f32; 3] {
    let d = l - lum(c);
    clip_color([c[0] + d, c[1] + d, c[2] + d])
}
fn sat(c: [f32; 3]) -> f32 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}
fn set_sat(c: [f32; 3], s: f32) -> [f32; 3] {
    let mx = c[0].max(c[1]).max(c[2]);
    let mn = c[0].min(c[1]).min(c[2]);
    let r = mx - mn;
    if r <= 1e-6 {
        return [0.0; 3];
    }
    [(c[0] - mn) * s / r, (c[1] - mn) * s / r, (c[2] - mn) * s / r]
}

pub fn blend_color(mode: u32, cb: [f32; 3], cs: [f32; 3]) -> [f32; 3] {
    match mode {
        21 => set_lum(set_sat(cs, sat(cb)), lum(cb)),
        22 => set_lum(set_sat(cb, sat(cs)), lum(cb)),
        23 => set_lum(cs, lum(cb)),
        24 => set_lum(cb, lum(cs)),
        _ => [blend_channel(mode, cb[0], cs[0]), blend_channel(mode, cb[1], cs[1]), blend_channel(mode, cb[2], cs[2])],
    }
}

/// Input 0 = base (backdrop), input 1 = blend layer (source).
/// P0 mode index into [`BLEND_MODES`], P1 opacity.
pub const BLEND: Kernel = Kernel {
    name: "blend",
    inputs: 2,
    wgsl: r#"
fn screen1(a: f32, b: f32) -> f32 { return a + b - a * b; }
fn color_dodge(cb: f32, cs: f32) -> f32 {
    if (cb <= 0.0) { return 0.0; }
    if (cs >= 1.0) { return 1.0; }
    return min(cb / (1.0 - cs), 1.0);
}
fn color_burn(cb: f32, cs: f32) -> f32 {
    if (cb >= 1.0) { return 1.0; }
    if (cs <= 0.0) { return 0.0; }
    return 1.0 - min((1.0 - cb) / cs, 1.0);
}
fn hard_light(cb: f32, cs: f32) -> f32 {
    if (cs <= 0.5) { return cb * 2.0 * cs; }
    return screen1(cb, 2.0 * cs - 1.0);
}
fn soft_light(cb: f32, cs: f32) -> f32 {
    if (cs <= 0.5) { return cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb); }
    var d = sqrt(cb);
    if (cb <= 0.25) { d = ((16.0 * cb - 12.0) * cb + 4.0) * cb; }
    return cb + (2.0 * cs - 1.0) * (d - cb);
}
fn vivid_light(cb: f32, cs: f32) -> f32 {
    if (cs <= 0.5) { return color_burn(cb, 2.0 * cs); }
    return color_dodge(cb, 2.0 * cs - 1.0);
}
fn logic(cb: f32, cs: f32, op: u32) -> f32 {
    let a = u32(round(clamp(cb, 0.0, 1.0) * 255.0));
    let b = u32(round(clamp(cs, 0.0, 1.0) * 255.0));
    var r = a ^ b;
    if (op == 0u) { r = a & b; }
    if (op == 1u) { r = a | b; }
    return f32(r) / 255.0;
}
fn blend_channel(mode: u32, cb: f32, cs: f32) -> f32 {
    switch mode {
        case 2u: { return cb * cs; }
        case 3u: { return screen1(cb, cs); }
        case 4u: { return hard_light(cs, cb); }
        case 5u: { return hard_light(cb, cs); }
        case 6u: { return soft_light(cb, cs); }
        case 7u: { return color_dodge(cb, cs); }
        case 8u: { return min(cb + cs, 1.0); }
        case 9u: { return color_burn(cb, cs); }
        case 10u: { return max(cb + cs - 1.0, 0.0); }
        case 11u: { return vivid_light(cb, cs); }
        case 12u: { return clamp(cb + 2.0 * cs - 1.0, 0.0, 1.0); }
        case 13u: {
            if (cs <= 0.5) { return min(cb, 2.0 * cs); }
            return max(cb, 2.0 * cs - 1.0);
        }
        case 14u: {
            if (vivid_light(cb, cs) >= 0.5) { return 1.0; }
            return 0.0;
        }
        case 15u: {
            if (cs <= 0.0) { return 1.0; }
            return clamp(cb / cs, 0.0, 1.0);
        }
        case 16u: { return max(cb - cs, 0.0); }
        case 17u: { return abs(cb - cs); }
        case 18u: { return cb + cs - 2.0 * cb * cs; }
        case 19u: { return min(cb, cs); }
        case 20u: { return max(cb, cs); }
        case 25u: { return logic(cb, cs, 0u); }
        case 26u: { return logic(cb, cs, 1u); }
        case 27u: { return logic(cb, cs, 2u); }
        default: { return cs; }
    }
}
fn lum(c: vec3<f32>) -> f32 { return dot(c, vec3<f32>(0.3, 0.59, 0.11)); }
fn clip_color(c: vec3<f32>) -> vec3<f32> {
    let l = lum(c);
    let n = min(min(c.r, c.g), c.b);
    let x = max(max(c.r, c.g), c.b);
    var o = c;
    if (n < 0.0) { o = l + (o - l) * l / max(l - n, 1e-6); }
    if (x > 1.0) { o = l + (o - l) * (1.0 - l) / max(x - l, 1e-6); }
    return o;
}
fn set_lum(c: vec3<f32>, l: f32) -> vec3<f32> { return clip_color(c + (l - lum(c))); }
fn sat(c: vec3<f32>) -> f32 { return max(max(c.r, c.g), c.b) - min(min(c.r, c.g), c.b); }
fn set_sat(c: vec3<f32>, s: f32) -> vec3<f32> {
    let mx = max(max(c.r, c.g), c.b);
    let mn = min(min(c.r, c.g), c.b);
    let r = mx - mn;
    if (r <= 1e-6) { return vec3<f32>(0.0); }
    return (c - mn) * s / r;
}
fn blend_color(mode: u32, cb: vec3<f32>, cs: vec3<f32>) -> vec3<f32> {
    switch mode {
        case 21u: { return set_lum(set_sat(cs, sat(cb)), lum(cb)); }
        case 22u: { return set_lum(set_sat(cb, sat(cs)), lum(cb)); }
        case 23u: { return set_lum(cs, lum(cb)); }
        case 24u: { return set_lum(cb, lum(cs)); }
        default: {
            return vec3<f32>(blend_channel(mode, cb.r, cs.r), blend_channel(mode, cb.g, cs.g), blend_channel(mode, cb.b, cs.b));
        }
    }
}
fn kernel(p: vec2<f32>) -> vec4<f32> {
    let b = s0(p);
    let s = s1(p);
    let mode = u32(P(0u) + 0.5);
    var a_s = s.a * clamp(P(1u), 0.0, 1.0);
    let cb = unpremul(b);
    let cs = unpremul(s);
    var mixed = cs;
    if (mode == 1u) {
        let n = node(p);
        let h = f32(hash2(i32(floor(n.x)), i32(floor(n.y)), 7u)) / 4294967296.0;
        a_s = select(0.0, 1.0, h < a_s);
    } else if (mode != 0u) {
        mixed = (1.0 - b.a) * cs + b.a * blend_color(mode, cb, cs);
    }
    let rgb = a_s * mixed + (1.0 - a_s) * b.rgb;
    return clamp_premul(vec4<f32>(rgb, a_s + b.a * (1.0 - a_s)));
}
"#,
    cpu: |e, r| {
        let [s0, s1, ..] = r.s;
        r.fill(|i, x, y| {
            let b = s0[i];
            let s = s1[i];
            let mode = (e.p[0] + 0.5) as u32;
            let mut a_s = s[3] * e.p[1].clamp(0.0, 1.0);
            let cb = unpremul(b);
            let cs = unpremul(s);
            let mut mixed = cs;
            if mode == 1 {
                let (nx, ny) = e.node(x, y);
                let h = hash2(nx.floor() as i32, ny.floor() as i32, 7) as f32 / 4294967296.0;
                a_s = if h < a_s { 1.0 } else { 0.0 };
            } else if mode != 0 {
                let bc = blend_color(mode, cb, cs);
                for i in 0..3 {
                    mixed[i] = (1.0 - b[3]) * cs[i] + b[3] * bc[i];
                }
            }
            let mut o = [0.0; 4];
            for i in 0..3 {
                o[i] = a_s * mixed[i] + (1.0 - a_s) * b[i];
            }
            o[3] = a_s + b[3] * (1.0 - a_s);
            clamp_premul(o)
        })
    },
};

// ---------------------------------------------------------------------------
// Procedural generators (no inputs). Coordinates are normalised against the
// requested extent so results are resolution independent.
// ---------------------------------------------------------------------------

pub const WGSL_NOISE: &str = r#"
fn hash_u(x: u32) -> u32 {
    let v = x * 747796405u + 2891336453u;
    let w = ((v >> ((v >> 28u) + 4u)) ^ v) * 277803737u;
    return (w >> 22u) ^ w;
}
fn hash2(ix: i32, iy: i32, seed: u32) -> u32 {
    return hash_u(bitcast<u32>(ix) ^ hash_u(bitcast<u32>(iy) ^ hash_u(seed)));
}
fn hashf(ix: i32, iy: i32, seed: u32) -> f32 { return f32(hash2(ix, iy, seed) >> 8u) / 16777216.0; }
fn grad(h: u32) -> vec2<f32> {
    switch (h & 7u) {
        case 0u: { return vec2<f32>(1.0, 0.0); }
        case 1u: { return vec2<f32>(-1.0, 0.0); }
        case 2u: { return vec2<f32>(0.0, 1.0); }
        case 3u: { return vec2<f32>(0.0, -1.0); }
        case 4u: { return vec2<f32>(0.70710678, 0.70710678); }
        case 5u: { return vec2<f32>(-0.70710678, 0.70710678); }
        case 6u: { return vec2<f32>(0.70710678, -0.70710678); }
        default: { return vec2<f32>(-0.70710678, -0.70710678); }
    }
}
fn fade(t: f32) -> f32 { return t * t * t * (t * (t * 6.0 - 15.0) + 10.0); }
fn perlin(q: vec2<f32>, seed: u32) -> f32 {
    let i = floor(q);
    let f = q - i;
    let ix = i32(i.x);
    let iy = i32(i.y);
    let a = dot(grad(hash2(ix, iy, seed)), f);
    let b = dot(grad(hash2(ix + 1, iy, seed)), f - vec2<f32>(1.0, 0.0));
    let c = dot(grad(hash2(ix, iy + 1, seed)), f - vec2<f32>(0.0, 1.0));
    let d = dot(grad(hash2(ix + 1, iy + 1, seed)), f - vec2<f32>(1.0, 1.0));
    let u = fade(f.x);
    let v = fade(f.y);
    return mix(mix(a, b, u), mix(c, d, u), v) * 1.4142135;
}
"#;

#[inline]
pub fn hash_u(x: u32) -> u32 {
    let v = x.wrapping_mul(747796405).wrapping_add(2891336453);
    let w = ((v >> ((v >> 28) + 4)) ^ v).wrapping_mul(277803737);
    (w >> 22) ^ w
}
#[inline]
pub fn hash2(ix: i32, iy: i32, seed: u32) -> u32 {
    hash_u((ix as u32) ^ hash_u((iy as u32) ^ hash_u(seed)))
}
#[inline]
fn hashf(ix: i32, iy: i32, seed: u32) -> f32 {
    (hash2(ix, iy, seed) >> 8) as f32 / 16777216.0
}
const D: f32 = 0.70710678;
const GRADS: [(f32, f32); 8] = [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0), (D, D), (-D, D), (D, -D), (-D, -D)];
#[inline]
fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}
#[inline]
fn perlin(qx: f32, qy: f32, seed: u32) -> f32 {
    let (ix, iy) = (qx.floor(), qy.floor());
    let (fx, fy) = (qx - ix, qy - iy);
    let (ix, iy) = (ix as i32, iy as i32);
    let g = |x: i32, y: i32, dx: f32, dy: f32| {
        let (gx, gy) = GRADS[(hash2(x, y, seed) & 7) as usize];
        gx * dx + gy * dy
    };
    let a = g(ix, iy, fx, fy);
    let b = g(ix + 1, iy, fx - 1.0, fy);
    let c = g(ix, iy + 1, fx, fy - 1.0);
    let d = g(ix + 1, iy + 1, fx - 1.0, fy - 1.0);
    let u = fade(fx);
    let v = fade(fy);
    let top = a + (b - a) * u;
    let bot = c + (d - c) * u;
    (top + (bot - top) * v) * std::f32::consts::SQRT_2
}

#[inline]
fn mix_px(a: Px, b: Px, t: f32) -> Px {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t, a[3] + (b[3] - a[3]) * t]
}

/// P0..3 color.
pub const SOLID: Kernel = Kernel {
    name: "solid",
    inputs: 0,
    wgsl: "fn kernel(p: vec2<f32>) -> vec4<f32> { return color_param(0u); }",
    cpu: |e, r| r.fill(|_, _, _| e.color(0)),
};

/// P0..3 color A, P4..7 color B, P8,P9 start (fraction of extent), P10,P11 end.
pub const LINEAR_GRADIENT: Kernel = Kernel {
    name: "linear_gradient",
    inputs: 0,
    wgsl: r#"fn kernel(p: vec2<f32>) -> vec4<f32> {
        let n = node(p) / extent();
        let a = vec2<f32>(P(8u), P(9u));
        let d = vec2<f32>(P(10u), P(11u)) - a;
        let t = dot(n - a, d) / max(dot(d, d), 1e-9);
        return mix(color_param(0u), color_param(4u), clamp(t, 0.0, 1.0));
    }"#,
    cpu: |e, r| r.fill(|_, x, y| {
        let (nx, ny) = e.node(x, y);
        let (nx, ny) = (nx / e.extent[0], ny / e.extent[1]);
        let (ax, ay) = (e.p[8], e.p[9]);
        let (dx, dy) = (e.p[10] - ax, e.p[11] - ay);
        let t = ((nx - ax) * dx + (ny - ay) * dy) / (dx * dx + dy * dy).max(1e-9);
        mix_px(e.color(0), e.color(4), t)
    }),
};

/// P0..3 inner color, P4..7 outer color, P8,P9 center (fraction of extent),
/// P10 radius (fraction of the smaller dimension).
pub const RADIAL_GRADIENT: Kernel = Kernel {
    name: "radial_gradient",
    inputs: 0,
    wgsl: r#"fn kernel(p: vec2<f32>) -> vec4<f32> {
        let e = extent();
        let c = vec2<f32>(P(8u), P(9u)) * e;
        let r = max(P(10u) * min(e.x, e.y), 1e-6);
        let t = length(node(p) - c) / r;
        return mix(color_param(0u), color_param(4u), clamp(t, 0.0, 1.0));
    }"#,
    cpu: |e, r| r.fill(|_, x, y| {
        let (nx, ny) = e.node(x, y);
        let (cx, cy) = (e.p[8] * e.extent[0], e.p[9] * e.extent[1]);
        let r = (e.p[10] * e.extent[0].min(e.extent[1])).max(1e-6);
        let t = ((nx - cx).powi(2) + (ny - cy).powi(2)).sqrt() / r;
        mix_px(e.color(0), e.color(4), t)
    }),
};

/// P0..3 color A, P4..7 color B, P8 cell size (fraction of smaller dim).
pub const CHECKER: Kernel = Kernel {
    name: "checker",
    inputs: 0,
    wgsl: r#"fn kernel(p: vec2<f32>) -> vec4<f32> {
        let e = extent();
        let s = max(P(8u) * min(e.x, e.y), 1e-3);
        let q = floor(node(p) / s);
        let k = i32(q.x) + i32(q.y);
        if ((k & 1) == 0) { return color_param(0u); }
        return color_param(4u);
    }"#,
    cpu: |e, r| r.fill(|_, x, y| {
        let (nx, ny) = e.node(x, y);
        let s = (e.p[8] * e.extent[0].min(e.extent[1])).max(1e-3);
        let k = (nx / s).floor() as i32 + (ny / s).floor() as i32;
        if k & 1 == 0 { e.color(0) } else { e.color(4) }
    }),
};

/// Fractal Perlin noise.
/// P0..3 color A, P4..7 color B, P8 feature size (fraction of smaller dim),
/// P9 octaves, P10 persistence, P11 lacunarity, P12 seed, P13 type
/// (0 fBm, 1 turbulence, 2 ridged), P14 contrast.
pub const NOISE: Kernel = Kernel {
    name: "noise",
    inputs: 0,
    wgsl: r#"fn kernel(p: vec2<f32>) -> vec4<f32> {
        let e = extent();
        var q = node(p) / max(P(8u) * min(e.x, e.y), 1e-3);
        let octaves = u32(clamp(P(9u), 1.0, 12.0));
        let ty = u32(P(13u) + 0.5);
        var amp = 1.0;
        var total = 0.0;
        var sum = 0.0;
        for (var o = 0u; o < octaves; o++) {
            var n = perlin(q, u32(P(12u)) + o * 1013u);
            if (ty == 1u) { n = abs(n) * 2.0 - 1.0; }
            if (ty == 2u) { n = (1.0 - abs(n)) * 2.0 - 1.0; }
            sum += n * amp;
            total += amp;
            amp *= P(10u);
            q *= P(11u);
        }
        let v = sum / max(total, 1e-6);
        let t = 0.5 + 0.5 * clamp(v * P(14u), -1.0, 1.0);
        return mix(color_param(0u), color_param(4u), t);
    }"#,
    cpu: |e, r| r.fill(|_, x, y| {
        let (nx, ny) = e.node(x, y);
        let s = (e.p[8] * e.extent[0].min(e.extent[1])).max(1e-3);
        let (mut qx, mut qy) = (nx / s, ny / s);
        let octaves = e.p[9].clamp(1.0, 12.0) as u32;
        let ty = (e.p[13] + 0.5) as u32;
        let (mut amp, mut total, mut sum) = (1.0f32, 0.0f32, 0.0f32);
        for o in 0..octaves {
            let mut n = perlin(qx, qy, (e.p[12] as u32).wrapping_add(o.wrapping_mul(1013)));
            if ty == 1 {
                n = n.abs() * 2.0 - 1.0;
            }
            if ty == 2 {
                n = (1.0 - n.abs()) * 2.0 - 1.0;
            }
            sum += n * amp;
            total += amp;
            amp *= e.p[10];
            qx *= e.p[11];
            qy *= e.p[11];
        }
        let v = sum / total.max(1e-6);
        let t = 0.5 + 0.5 * (v * e.p[14]).clamp(-1.0, 1.0);
        mix_px(e.color(0), e.color(4), t)
    }),
};

/// Worley / Voronoi cells.
/// P0..3 color A, P4..7 color B, P8 cell size (fraction of smaller dim),
/// P9 jitter, P10 seed, P11 mode (0 distance, 1 flat cells, 2 edges).
pub const VORONOI: Kernel = Kernel {
    name: "voronoi",
    inputs: 0,
    wgsl: r#"fn kernel(p: vec2<f32>) -> vec4<f32> {
        let e = extent();
        let s = max(P(8u) * min(e.x, e.y), 1e-3);
        let q = node(p) / s;
        let cell = floor(q);
        let seed = u32(P(10u));
        let jitter = clamp(P(9u), 0.0, 1.0);
        var f1 = 1e9;
        var f2 = 1e9;
        var best = vec2<i32>(0);
        for (var j = -1; j <= 1; j++) {
            for (var i = -1; i <= 1; i++) {
                let cx = i32(cell.x) + i;
                let cy = i32(cell.y) + j;
                let pt = vec2<f32>(f32(cx), f32(cy)) + 0.5 + (vec2<f32>(hashf(cx, cy, seed), hashf(cx, cy, seed + 1u)) - 0.5) * jitter;
                let d = length(q - pt);
                if (d < f1) { f2 = f1; f1 = d; best = vec2<i32>(cx, cy); } else if (d < f2) { f2 = d; }
            }
        }
        let mode = u32(P(11u) + 0.5);
        var t = f1;
        if (mode == 1u) { t = hashf(best.x, best.y, seed + 2u); }
        if (mode == 2u) { t = 1.0 - smoothstep(0.0, 0.08, f2 - f1); }
        return mix(color_param(0u), color_param(4u), clamp(t, 0.0, 1.0));
    }"#,
    cpu: |e, r| r.fill(|_, x, y| {
        let (nx, ny) = e.node(x, y);
        let s = (e.p[8] * e.extent[0].min(e.extent[1])).max(1e-3);
        let (qx, qy) = (nx / s, ny / s);
        let (cx0, cy0) = (qx.floor() as i32, qy.floor() as i32);
        let seed = e.p[10] as u32;
        let jitter = e.p[9].clamp(0.0, 1.0);
        let (mut f1, mut f2, mut best) = (1e9f32, 1e9f32, (0, 0));
        for j in -1..=1 {
            for i in -1..=1 {
                let (cx, cy) = (cx0 + i, cy0 + j);
                let px = cx as f32 + 0.5 + (hashf(cx, cy, seed) - 0.5) * jitter;
                let py = cy as f32 + 0.5 + (hashf(cx, cy, seed.wrapping_add(1)) - 0.5) * jitter;
                let d = ((qx - px).powi(2) + (qy - py).powi(2)).sqrt();
                if d < f1 {
                    f2 = f1;
                    f1 = d;
                    best = (cx, cy);
                } else if d < f2 {
                    f2 = d;
                }
            }
        }
        let t = match (e.p[11] + 0.5) as u32 {
            1 => hashf(best.0, best.1, seed.wrapping_add(2)),
            2 => {
                let x = ((f2 - f1) / 0.08).clamp(0.0, 1.0);
                1.0 - x * x * (3.0 - 2.0 * x)
            }
            _ => f1,
        };
        mix_px(e.color(0), e.color(4), t)
    }),
};

/// Mandelbrot set with a cosine palette.
/// P0,P1 center (complex plane), P2 zoom, P3 max iterations,
/// P4..6 palette phase, P7 interior brightness.
pub const MANDELBROT: Kernel = Kernel {
    name: "mandelbrot",
    inputs: 0,
    wgsl: r#"fn kernel(p: vec2<f32>) -> vec4<f32> {
        let e = extent();
        let c = vec2<f32>(P(0u), P(1u)) + (node(p) - e * 0.5) / min(e.x, e.y) * (3.0 / max(P(2u), 1e-6));
        var z = vec2<f32>(0.0);
        let maxi = u32(clamp(P(3u), 1.0, 5000.0));
        var i = 0u;
        loop {
            if (i >= maxi || dot(z, z) > 256.0) { break; }
            z = vec2<f32>(z.x * z.x - z.y * z.y, 2.0 * z.x * z.y) + c;
            i++;
        }
        if (i >= maxi) { return vec4<f32>(vec3<f32>(P(7u)), 1.0); }
        let sm = f32(i) - log2(log2(dot(z, z))) + 4.0;
        let t = sm / 32.0;
        let col = 0.5 + 0.5 * cos(6.2831853 * (vec3<f32>(t) + vec3<f32>(P(4u), P(5u), P(6u))));
        return vec4<f32>(col, 1.0);
    }"#,
    cpu: |e, r| r.fill(|_, x, y| {
        let (nx, ny) = e.node(x, y);
        let md = e.extent[0].min(e.extent[1]);
        let k = 3.0 / e.p[2].max(1e-6);
        let cx = e.p[0] + (nx - e.extent[0] * 0.5) / md * k;
        let cy = e.p[1] + (ny - e.extent[1] * 0.5) / md * k;
        let (mut zx, mut zy) = (0.0f32, 0.0f32);
        let maxi = e.p[3].clamp(1.0, 5000.0) as u32;
        let mut i = 0;
        while i < maxi && zx * zx + zy * zy <= 256.0 {
            let t = zx * zx - zy * zy + cx;
            zy = 2.0 * zx * zy + cy;
            zx = t;
            i += 1;
        }
        if i >= maxi {
            let v = e.p[7];
            return [v, v, v, 1.0];
        }
        let sm = i as f32 - (zx * zx + zy * zy).log2().log2() + 4.0;
        let t = sm / 32.0;
        let f = |ph: f32| 0.5 + 0.5 * (std::f32::consts::TAU * (t + ph)).cos();
        [f(e.p[4]), f(e.p[5]), f(e.p[6]), 1.0]
    }),
};

pub const ALL: &[&Kernel] = &[
    &RESAMPLE, &INVERT, &OVER, &MASK, &SPLIT, &REFLECT, &BLEND, &SOLID, &LINEAR_GRADIENT, &RADIAL_GRADIENT, &CHECKER, &NOISE, &VORONOI, &MANDELBROT,
];

/// Runs a kernel on the CPU, parallelised over rows.
pub fn run_cpu(k: &Kernel, env: &Env, w: u32, h: u32) -> CpuTile {
    let mut t = CpuTile::new(w, h);
    let w = w as usize;
    if w == 0 {
        return t;
    }
    let clear = vec![CLEAR; w];
    t.data.par_chunks_mut(w).enumerate().for_each_init(
        || vec![vec![CLEAR; w]; MAX_INPUTS],
        |scratch, (j, out)| {
            let y = j as f32 + 0.5;
            // Inputs not on the output grid are resampled into scratch rows.
            for (inp, buf) in env.inputs.iter().zip(scratch.iter_mut()) {
                if let (Some(tile), None) = (inp.tile, inp.aligned_row(j, w)) {
                    inp.sample_row(tile, y, buf);
                }
            }
            let mut s: [&[Px]; MAX_INPUTS] = [&clear; MAX_INPUTS];
            for (n, inp) in env.inputs.iter().enumerate() {
                if inp.tile.is_some() {
                    s[n] = inp.aligned_row(j, w).unwrap_or(&scratch[n]);
                }
            }
            (k.cpu)(env, Row { y, s, out });
        },
    );
    t
}
