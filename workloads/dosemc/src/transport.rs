//! Photon transport with analogue sampling; electrons deposit along a straight CSDA path
//! (v1: no multiple scattering, no bremsstrahlung). Good enough for build-up and interface
//! trends; a condensed-history electron model is the v2 upgrade.

use crate::phantom::Phantom;
use crate::physics::{water_csda_range, water_mu_rho, ME};
use crate::rng::Rng;

pub struct Scorer {
    pub dose: Vec<f64>,  // energy deposited per z-voxel (MeV)
    pub dose2: Vec<f64>, // sum of squares per history batch (for uncertainty)
    pub histories: u64,
}

impl Scorer {
    pub fn new(n: usize) -> Self {
        Scorer { dose: vec![0.0; n], dose2: vec![0.0; n], histories: 0 }
    }
    pub fn merge(&mut self, o: &Scorer) {
        for i in 0..self.dose.len() {
            self.dose[i] += o.dose[i];
            self.dose2[i] += o.dose2[i];
        }
        self.histories += o.histories;
    }
}

struct Particle {
    x: f64, y: f64, z: f64,
    u: f64, v: f64, w: f64, // direction cosines
    e: f64,                 // photon energy or electron kinetic energy (MeV)
}

const E_CUT_PHOTON: f64 = 0.010;
const E_CUT_ELECTRON: f64 = 0.010;

/// Deposit an electron's kinetic energy along a straight line of CSDA range, split across
/// the voxels it crosses (density-scaled), stopping at the phantom edge.
#[inline]
fn deposit_electron(p: &Particle, ph: &Phantom, dep: &mut [f64]) {
    let mut t = p.e;
    if t <= E_CUT_ELECTRON {
        // local deposition
        if ph.inside(p.x, p.y, p.z) {
            let i = (p.z / ph.dz_cm) as usize;
            if i < dep.len() { dep[i] += t; }
        }
        return;
    }
    let range_g = water_csda_range(t); // g/cm²
    let (mut x, mut y, mut z) = (p.x, p.y, p.z);
    let step = ph.dz_cm * 0.5;
    let mut remaining_g = range_g;
    while remaining_g > 0.0 && t > 0.0 {
        if !ph.inside(x, y, z) { return; }
        let rho = ph.density_at(z);
        let ds = step.min(remaining_g / rho);
        let frac = (ds * rho) / range_g;
        let de = (p.e * frac).min(t);
        let i = (z / ph.dz_cm) as usize;
        if i < dep.len() { dep[i] += de; }
        t -= de;
        remaining_g -= ds * rho;
        x += p.u * ds; y += p.v * ds; z += p.w * ds;
    }
}

#[inline]
fn rotate(u: f64, v: f64, w: f64, cos_t: f64, phi: f64) -> (f64, f64, f64) {
    let sin_t = (1.0 - cos_t * cos_t).max(0.0).sqrt();
    let (sp, cp) = phi.sin_cos();
    if w.abs() > 0.999_999 {
        let s = if w > 0.0 { 1.0 } else { -1.0 };
        return (sin_t * cp, s * sin_t * sp, s * cos_t);
    }
    let r = (1.0 - w * w).sqrt();
    (
        u * cos_t + sin_t * (u * w * cp - v * sp) / r,
        v * cos_t + sin_t * (v * w * cp + u * sp) / r,
        w * cos_t - r * sin_t * cp,
    )
}

/// Klein–Nishina sampling (Kahn's method). Returns (scattered photon energy, cos θ).
#[inline]
fn sample_compton(e: f64, rng: &mut Rng) -> (f64, f64) {
    let k = e / ME;
    let eps_min = 1.0 / (1.0 + 2.0 * k);
    let a1 = -(eps_min.ln());
    let a2 = 0.5 * (1.0 - eps_min * eps_min);
    loop {
        let (eps, cos_t);
        if rng.uniform() < a1 / (a1 + a2) {
            eps = eps_min.powf(rng.uniform());
        } else {
            let r = rng.uniform();
            eps = (eps_min * eps_min + r * (1.0 - eps_min * eps_min)).sqrt();
        }
        let t = (1.0 - eps) / (k * eps);
        cos_t = 1.0 - t;
        let sin2 = t * (2.0 - t);
        let g = 1.0 - eps * sin2 / (1.0 + eps * eps);
        if rng.uniform() <= g {
            return (e * eps, cos_t);
        }
    }
}

pub fn run_histories(
    ph: &Phantom, e0: f64, n: u64, seed: u64, stream: u64, field_half_cm: f64,
) -> Scorer {
    let mut rng = Rng::new(seed, stream);
    let nv = ph.n_voxels();
    let mut sc = Scorer::new(nv);
    let mut dep = vec![0.0f64; nv];
    let mut stack: Vec<Particle> = Vec::with_capacity(16);

    for _ in 0..n {
        for d in dep.iter_mut() { *d = 0.0; }
        // broad parallel beam, uniform over the field, entering at z=0
        let x0 = (2.0 * rng.uniform() - 1.0) * field_half_cm;
        let y0 = (2.0 * rng.uniform() - 1.0) * field_half_cm;
        stack.push(Particle { x: x0, y: y0, z: 0.0, u: 0.0, v: 0.0, w: 1.0, e: e0 });

        while let Some(mut p) = stack.pop() {
            loop {
                if p.e <= E_CUT_PHOTON || !ph.inside(p.x, p.y, p.z) { break; }
                let (tau, sig, kap) = water_mu_rho(p.e);
                let mu_rho = tau + sig + kap;
                // track to next interaction through slabs of different density
                let mut path_g = -rng.uniform().ln() / mu_rho; // g/cm² to travel
                let mut interacted = false;
                while path_g > 0.0 {
                    if !ph.inside(p.x, p.y, p.z) { break; }
                    let rho = ph.density_at(p.z);
                    let db = ph.dist_to_boundary(p.z, p.w);
                    let d_int = path_g / rho;
                    if d_int < db {
                        p.x += p.u * d_int; p.y += p.v * d_int; p.z += p.w * d_int;
                        path_g = 0.0;
                        interacted = true;
                    } else {
                        let d = db + 1e-7;
                        p.x += p.u * d; p.y += p.v * d; p.z += p.w * d;
                        path_g -= db * rho;
                    }
                }
                if !interacted || !ph.inside(p.x, p.y, p.z) { break; }

                let r = rng.uniform() * mu_rho;
                if r < tau {
                    // photoelectric: all energy to an electron (binding energy ignored)
                    let cos_t = 2.0 * rng.uniform() - 1.0;
                    let phi = 6.283_185_307_179_586 * rng.uniform();
                    let (u, v, w) = rotate(p.u, p.v, p.w, cos_t, phi);
                    deposit_electron(&Particle { x: p.x, y: p.y, z: p.z, u, v, w, e: p.e }, ph, &mut dep);
                    break;
                } else if r < tau + sig {
                    let (e_sc, cos_t) = sample_compton(p.e, &mut rng);
                    let phi = 6.283_185_307_179_586 * rng.uniform();
                    let t_el = p.e - e_sc;
                    // electron direction from momentum conservation
                    let k = p.e / ME;
                    let cos_e = ((1.0 + k) * ((1.0 - cos_t) / (2.0 + k * (1.0 - cos_t))).sqrt()).clamp(-1.0, 1.0);
                    let (ue, ve, we) = rotate(p.u, p.v, p.w, cos_e, phi + std::f64::consts::PI);
                    deposit_electron(&Particle { x: p.x, y: p.y, z: p.z, u: ue, v: ve, w: we, e: t_el }, ph, &mut dep);
                    let (u, v, w) = rotate(p.u, p.v, p.w, cos_t, phi);
                    p.u = u; p.v = v; p.w = w; p.e = e_sc;
                } else {
                    // pair production: kinetic energy split uniformly, both deposited along the photon direction;
                    // positron annihilates at rest into two back-to-back 0.511 MeV photons
                    let t_tot = p.e - 2.0 * ME;
                    let f = rng.uniform();
                    deposit_electron(&Particle { x: p.x, y: p.y, z: p.z, u: p.u, v: p.v, w: p.w, e: t_tot * f }, ph, &mut dep);
                    deposit_electron(&Particle { x: p.x, y: p.y, z: p.z, u: p.u, v: p.v, w: p.w, e: t_tot * (1.0 - f) }, ph, &mut dep);
                    let cos_t = 2.0 * rng.uniform() - 1.0;
                    let phi = 6.283_185_307_179_586 * rng.uniform();
                    let (u, v, w) = rotate(0.0, 0.0, 1.0, cos_t, phi);
                    stack.push(Particle { x: p.x, y: p.y, z: p.z, u, v, w, e: ME });
                    stack.push(Particle { x: p.x, y: p.y, z: p.z, u: -u, v: -v, w: -w, e: ME });
                    break;
                }
            }
        }
        for i in 0..nv {
            sc.dose[i] += dep[i];
            sc.dose2[i] += dep[i] * dep[i];
        }
        sc.histories += 1;
    }
    sc
}
