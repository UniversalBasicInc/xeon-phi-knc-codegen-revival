//! Lane-generic arithmetic for the solver, written for the Xeon Phi (KNC) vector unit.
//!
//! KNC has no vector divide, sqrt, sin or pow. The toolchain lowers a vector division to a scalar
//! runtime call per lane, and `sin`/`powf` are libm calls per lane; on an in-order core at 1.1 GHz
//! those dominated the first card run. Everything here is add/mul/fma/compare/select plus 32-bit
//! integer lane ops, all of which lower to native 512-bit instructions (vector_fp_fma is in the
//! toolchain capability record; division and sqrt are explicitly "scalar runtime fallback").
//!
//! The [`Lane`] trait is implemented for `f64` and `f64x8` so the model and the solvers are written
//! once and compiled for both. The scalar path performs the identical operation sequence, so the
//! scalar and eight-lane solvers stay bit-identical and either validates the other.

use std::ops::{Add, Mul, Neg, Sub};
use std::simd::{f64x8, i32x16, i32x8, prelude::*, simd_swizzle, StdFloat};

pub type V = f64x8;

pub trait Lane: Copy + Add<Output = Self> + Sub<Output = Self> + Mul<Output = Self> + Neg<Output = Self> {
    fn splat(x: f64) -> Self;
    /// self * a + b, single rounding
    fn mul_add(self, a: Self, b: Self) -> Self;
    fn abs(self) -> Self;
    fn max(self, o: Self) -> Self;
    fn min(self, o: Self) -> Self;
    /// `if self > o { a } else { b }`
    fn select_gt(self, o: Self, a: Self, b: Self) -> Self;
    /// |self| with the sign of `sign`, via the sign bit (the KNC backend has no copysign lowering)
    #[inline(always)]
    fn copysign(self, sign: Self) -> Self {
        let hi = (self.hi_words() & Self::wsplat(0x7FFF_FFFF)) | (sign.hi_words() & Self::wsplat(0x8000_0000u32 as i32));
        Self::from_words(self.lo_words(), hi)
    }
    /// bit pattern as 32-bit words (lo, hi) per lane
    fn from_words(lo: Self::W, hi: Self::W) -> Self;
    fn hi_words(self) -> Self::W;
    fn lo_words(self) -> Self::W;
    /// i32 words → f64 lanes (exact for |x| < 2^53)
    fn words_to_f64(w: Self::W) -> Self;
    type W: Copy + Add<Output = Self::W> + Sub<Output = Self::W> + std::ops::BitAnd<Output = Self::W> + std::ops::BitOr<Output = Self::W> + std::ops::Shl<Output = Self::W> + std::ops::Shr<Output = Self::W>;
    fn wsplat(x: i32) -> Self::W;
}

impl Lane for f64 {
    type W = i32;
    #[inline(always)] fn splat(x: f64) -> f64 { x }
    #[inline(always)] fn mul_add(self, a: f64, b: f64) -> f64 { f64::mul_add(self, a, b) }
    #[inline(always)] fn abs(self) -> f64 { f64::abs(self) }
    #[inline(always)] fn max(self, o: f64) -> f64 { f64::max(self, o) }
    #[inline(always)] fn min(self, o: f64) -> f64 { f64::min(self, o) }
    #[inline(always)] fn select_gt(self, o: f64, a: f64, b: f64) -> f64 { if self > o { a } else { b } }
    #[inline(always)] fn from_words(lo: i32, hi: i32) -> f64 { f64::from_bits(((hi as u32 as u64) << 32) | (lo as u32 as u64)) }
    #[inline(always)] fn hi_words(self) -> i32 { (self.to_bits() >> 32) as u32 as i32 }
    #[inline(always)] fn lo_words(self) -> i32 { self.to_bits() as u32 as i32 }
    #[inline(always)] fn words_to_f64(w: i32) -> f64 { w as f64 }
    #[inline(always)] fn wsplat(x: i32) -> i32 { x }
}

impl Lane for f64x8 {
    type W = i32x8;
    #[inline(always)] fn splat(x: f64) -> V { V::splat(x) }
    #[inline(always)] fn mul_add(self, a: V, b: V) -> V { StdFloat::mul_add(self, a, b) }
    #[inline(always)] fn abs(self) -> V { SimdFloat::abs(self) }
    #[inline(always)] fn max(self, o: V) -> V { self.simd_max(o) }
    #[inline(always)] fn min(self, o: V) -> V { self.simd_min(o) }
    #[inline(always)] fn select_gt(self, o: V, a: V, b: V) -> V { self.simd_gt(o).select(a, b) }
    #[inline(always)] fn from_words(lo: i32x8, hi: i32x8) -> V {
        let w: i32x16 = simd_swizzle!(lo, hi, [0, 8, 1, 9, 2, 10, 3, 11, 4, 12, 5, 13, 6, 14, 7, 15]);
        unsafe { std::mem::transmute(w) }
    }
    #[inline(always)] fn hi_words(self) -> i32x8 { let w: i32x16 = unsafe { std::mem::transmute(self) }; simd_swizzle!(w, [1, 3, 5, 7, 9, 11, 13, 15]) }
    #[inline(always)] fn lo_words(self) -> i32x8 { let w: i32x16 = unsafe { std::mem::transmute(self) }; simd_swizzle!(w, [0, 2, 4, 6, 8, 10, 12, 14]) }
    #[inline(always)] fn words_to_f64(w: i32x8) -> V { w.cast::<f64>() }
    #[inline(always)] fn wsplat(x: i32) -> i32x8 { i32x8::splat(x) }
}

/// 1/d for d > 0 (normal): bit-trick seed, five fused Newton steps. Max error 1 ulp.
#[inline(always)]
pub fn rcp<T: Lane>(d: T) -> T {
    let one = T::splat(1.0);
    let seed_hi = T::wsplat(0x7FDE_6238) - d.hi_words();
    let mut r = T::from_words(T::wsplat(0), seed_hi);
    for _ in 0..5 {
        let e = (-d).mul_add(r, one);
        r = r.mul_add(e, r);
    }
    r
}

/// n/d for d > 0: reciprocal, then one fused correction. Within 1 ulp of the IEEE quotient.
#[inline(always)]
pub fn div<T: Lane>(n: T, d: T) -> T {
    let r = rcp(d);
    let q = n * r;
    let rem = (-q).mul_add(d, n);
    rem.mul_add(r, q)
}

const BIG: f64 = 6755399441055744.0; // 1.5 * 2^52: (x + BIG) - BIG rounds x to the nearest integer

/// sin(pi * x), |x| < 2^50. fdlibm kernel polynomials on a quarter period.
#[inline(always)]
pub fn sinpi<T: Lane>(x: T) -> T {
    let s = T::splat;
    let big = s(BIG);
    let k2 = (x * s(0.5) + big) - big;           // nearest integer to x/2
    let u = x - (k2 + k2);                       // [-1, 1], exact
    let ua = u.abs();
    let r = ua.select_gt(s(0.5), s(1.0).copysign(u) - u, u); // [-0.5, 0.5], sin(pi r) = sin(pi u)
    let ra = r.abs();
    // sin kernel on |pi r| <= pi/4
    let xs = r * s(std::f64::consts::PI);
    let z = xs * xs;
    let v = z * xs;
    let rr = z.mul_add(z.mul_add(s(2.75573137070700676789e-06), s(-1.98412698298579493134e-04)), s(8.33333333332248946124e-03));
    let rr = (z * z * z).mul_add(z.mul_add(s(1.58969099521155010221e-10), s(-2.50507602534068634195e-08)), rr);
    let sin_k = v.mul_add(z.mul_add(rr, s(-1.66666666666666324348e-01)), xs);
    // cos kernel on pi (0.5 - |r|) <= pi/4
    let xc = (s(0.5) - ra) * s(std::f64::consts::PI);
    let z = xc * xc;
    let w = z * z;
    let rr = z.mul_add(z.mul_add(s(2.48015872894767294178e-05), s(-1.38888888888741095749e-03)), s(4.16666666666666019037e-02));
    let rr = (w * w).mul_add(z.mul_add(z.mul_add(s(-1.13596475577881948265e-11), s(2.08757232129817482790e-09)), s(-2.75573143513906633035e-07)), z * rr);
    let hz = s(0.5) * z;
    let w1 = s(1.0) - hz;
    let cos_k = w1 + ((s(1.0) - w1) - hz + z * rr);
    ra.select_gt(s(0.25), cos_k.copysign(r), sin_k)
}

/// cos(pi * x), |x| < 2^50: cos(pi x) = sin(pi (x + 0.5)) (x + 0.5 is exact for |x| < 2^50).
#[inline(always)]
pub fn cospi<T: Lane>(x: T) -> T { sinpi(x + T::splat(0.5)) }

/// d/dt of `mu_e_cyclic`: 2e4 * 160 * s^159 * cos(pi x) * pi/28, with s^159 = s^128 * s^16 * s^8 * s^4 * s^2 * s.
#[inline(always)]
pub fn mu_e_cyclic_dt<T: Lane>(t: T) -> T {
    let x = (t + T::splat(12.0)) * T::splat(1.0 / 28.0);
    let s1 = sinpi(x);
    let c = cospi(x);
    let s2 = s1 * s1; let s4 = s2 * s2; let s8 = s4 * s4; let s16 = s8 * s8;
    let s32 = s16 * s16; let s64 = s32 * s32; let s128 = s64 * s64;
    let s159 = s128 * s16 * s8 * s4 * s2 * s1;
    T::splat(2e4 * 160.0 * std::f64::consts::PI / 28.0) * s159 * c
}

/// Cyclic menstrual influx, eq. 2.9 as coded by the authors: 2e4 * sin(pi (t+12)/28)^160.
/// The 160th power is the square-and-multiply sequence of compiler-rt's __powidf2 (s^32 * s^128).
#[inline(always)]
pub fn mu_e_cyclic<T: Lane>(t: T) -> T {
    let x = (t + T::splat(12.0)) * T::splat(1.0 / 28.0);
    let s1 = sinpi(x);
    let s2 = s1 * s1; let s4 = s2 * s2; let s8 = s4 * s4; let s16 = s8 * s8;
    let s32 = s16 * s16; let s64 = s32 * s32; let s128 = s64 * s64;
    T::splat(2e4) * (s32 * s128)
}

/// log2(x) for normal x > 0. Relative error ~1e-14; used only by the step-size controller.
#[inline(always)]
pub fn log2<T: Lane>(x: T) -> T {
    let s = T::splat;
    let hi = x.hi_words();
    let e = T::words_to_f64(((hi >> T::wsplat(20)) & T::wsplat(0x7ff)) - T::wsplat(1023));
    let m = T::from_words(x.lo_words(), (hi & T::wsplat(0x000F_FFFF)) | T::wsplat(0x3FF0_0000)); // [1, 2)
    let sqrt2 = s(std::f64::consts::SQRT_2);
    let e = m.select_gt(sqrt2, e + s(1.0), e);
    let m = m.select_gt(sqrt2, m * s(0.5), m);                                   // [0.707, 1.414]
    let f = div(m - s(1.0), m + s(1.0));                                         // |f| <= 0.1716
    let f2 = f * f;
    let p = f2.mul_add(s(2.0 / 15.0), s(2.0 / 13.0));
    let p = f2.mul_add(p, s(2.0 / 11.0));
    let p = f2.mul_add(p, s(2.0 / 9.0));
    let p = f2.mul_add(p, s(2.0 / 7.0));
    let p = f2.mul_add(p, s(2.0 / 5.0));
    let p = f2.mul_add(p, s(2.0 / 3.0));
    let p = f2.mul_add(p, s(2.0));
    (f * p).mul_add(s(std::f64::consts::LOG2_E), e)
}

/// 2^y for |y| < 1000. Relative error ~1e-14.
#[inline(always)]
pub fn exp2<T: Lane>(y: T) -> T {
    let s = T::splat;
    let big = s(BIG);
    let nb = y + big;                       // integer part in the low word
    let n = nb - big;
    let f = y - n;                          // [-0.5, 0.5]
    let t = f * s(std::f64::consts::LN_2);  // |t| <= 0.347
    let mut p = s(1.0 / 39916800.0);        // 1/11!
    for &c in &[1.0 / 3628800.0, 1.0 / 362880.0, 1.0 / 40320.0, 1.0 / 5040.0, 1.0 / 720.0, 1.0 / 120.0, 1.0 / 24.0, 1.0 / 6.0, 0.5, 1.0, 1.0] {
        p = t.mul_add(p, s(c));
    }
    let hi = (nb.lo_words() + T::wsplat(1023)) << T::wsplat(20);
    let pow2n = T::from_words(T::wsplat(0), hi);
    p * pow2n
}

/// x^p for normal x > 0.
#[inline(always)]
pub fn powf<T: Lane>(x: T, p: T) -> T { exp2(p * log2(x)) }

/// y + hh * (c0 k0 + c1 k1 + ...) as one fused chain, identical in both solvers.
#[inline(always)]
pub fn comb<T: Lane, const K: usize>(y: T, hh: T, coef: [f64; K], ks: [T; K]) -> T {
    let mut acc = ks[0] * T::splat(coef[0]);
    for i in 1..K { acc = ks[i].mul_add(T::splat(coef[i]), acc); }
    hh.mul_add(acc, y)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rng(seed: &mut u64) -> f64 { *seed ^= *seed << 13; *seed ^= *seed >> 7; *seed ^= *seed << 17; (*seed >> 11) as f64 / 9007199254740992.0 }
    fn ulp_diff(a: f64, b: f64) -> u64 { (a.to_bits() as i64 - b.to_bits() as i64).unsigned_abs() }

    #[test]
    fn scalar_and_vector_are_bit_identical() {
        let mut s = 0x1234_5678_9abc_def1u64;
        for _ in 0..2000 {
            let xs: [f64; 8] = std::array::from_fn(|_| 10f64.powf(rng(&mut s) * 20.0 - 10.0));
            let ts: [f64; 8] = std::array::from_fn(|_| rng(&mut s) * 5000.0);
            let ps: [f64; 8] = std::array::from_fn(|i| if i % 2 == 0 { -0.2 } else { -0.25 });
            let (vx, vt, vp) = (V::from_array(xs), V::from_array(ts), V::from_array(ps));
            let (vr, vd, vs, vm, vl, ve, vpw) = (rcp(vx).to_array(), div(vt, vx).to_array(), sinpi(vt).to_array(), mu_e_cyclic(vt).to_array(), log2(vx).to_array(), exp2(vl_of(vx)).to_array(), powf(vx, vp).to_array());
            for l in 0..8 {
                assert_eq!(rcp(xs[l]).to_bits(), vr[l].to_bits(), "rcp");
                assert_eq!(div(ts[l], xs[l]).to_bits(), vd[l].to_bits(), "div");
                assert_eq!(sinpi(ts[l]).to_bits(), vs[l].to_bits(), "sinpi");
                assert_eq!(mu_e_cyclic(ts[l]).to_bits(), vm[l].to_bits(), "mu_e");
                assert_eq!(log2(xs[l]).to_bits(), vl[l].to_bits(), "log2");
                assert_eq!(exp2(log2(xs[l])).to_bits(), ve[l].to_bits(), "exp2");
                assert_eq!(powf(xs[l], ps[l]).to_bits(), vpw[l].to_bits(), "powf");
            }
        }
    }
    fn vl_of(x: V) -> V { log2(x) }

    #[test]
    fn accuracy_against_std() {
        let mut s = 0xdead_beef_cafe_f00du64;
        let (mut w_div, mut w_sin, mut w_mu, mut w_pow) = (0u64, 0f64, 0f64, 0f64);
        for _ in 0..200000 {
            let d = 10f64.powf(rng(&mut s) * 20.0 - 10.0);
            let n = 10f64.powf(rng(&mut s) * 20.0 - 10.0) * if rng(&mut s) < 0.5 { -1.0 } else { 1.0 };
            w_div = w_div.max(ulp_diff(div(n, d), n / d));
            let t = rng(&mut s) * 5000.0;
            // reference with exact range reduction: (PI * t).sin() itself loses ~1e-12 at t ~ 5000
            let red = |x: f64| x - 2.0 * (x / 2.0).round();
            w_sin = w_sin.max((sinpi(t) - (std::f64::consts::PI * red(t)).sin()).abs());
            let m_ref = 2e4 * (std::f64::consts::PI * red((t + 12.0) / 28.0)).sin().powi(160);
            if m_ref > 1e-200 { w_mu = w_mu.max(((mu_e_cyclic(t) - m_ref) / m_ref).abs()); }
            let p = if rng(&mut s) < 0.5 { -0.2 } else { -0.25 };
            w_pow = w_pow.max(((powf(d, p) - d.powf(p)) / d.powf(p)).abs());
        }
        eprintln!("worst: div {w_div} ulp, sinpi {w_sin:.2e} abs, mu_e {w_mu:.2e} rel, powf {w_pow:.2e} rel");
        assert!(w_div <= 1, "div {w_div} ulp");
        assert!(w_sin < 1e-15, "sinpi {w_sin}");
        // (t+12)*(1/28) vs the reference's (t+12)/28 differ by an ulp of the argument; the 160th
        // power amplifies that to ~1e-10 relative where mu_e is already ~1e-200 (no influx)
        assert!(w_mu < 1e-9, "mu_e {w_mu}");
        assert!(w_pow < 1e-12, "powf {w_pow}");
    }
}
