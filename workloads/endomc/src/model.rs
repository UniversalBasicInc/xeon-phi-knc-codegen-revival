//! Miller, Germano, Chenoweth, Holdsworth-Carson (2025), J R Soc Interface 22(229):20250076.
//! State: [M0, M1, M2, K0, KA, E0, EF, EA] (cells/mL). Time in days.
//! Transcribed from the authors' `fn_immune_ode_system.m` / `default_parameters.m` (CC-BY-4.0).

use crate::vmath::{self, Lane};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub mu_m: f64, pub mu_k: f64, pub eta_e: f64, pub eta_k: f64, pub eta_m: f64,
    pub delta_m: f64, pub delta_k: f64, pub delta_e0: f64, pub delta_e: f64, pub sigma: f64,
    pub mc: f64, pub kc: f64, pub ec: f64, pub cm1: f64, pub cm2: f64, pub cka: f64,
    pub beta2: f64, pub beta12: f64, pub beta21: f64, pub theta_m: f64, pub theta_k: f64,
    pub gamma: f64, pub rho_f: f64, pub mu_e: f64,
    // the three the paper varies
    pub omega: f64, pub beta1: f64, pub rho0: f64,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            mu_m: 1e4, mu_k: 3.5e3, eta_e: 1.0, eta_k: 0.02, eta_m: 0.2,
            delta_m: 0.02, delta_k: 0.02, delta_e0: 1.0, delta_e: 0.14, sigma: 1e-5,
            mc: 1e6, kc: 1e6, ec: 1e9, cm1: 1e5, cm2: 100.0, cka: 1e5,
            beta2: 1e-3, beta12: 5e-5, beta21: 5e-5, theta_m: 5e-8, theta_k: 0.7,
            gamma: 0.8, rho_f: 0.1, mu_e: 1260.0,
            omega: 1e-5, beta1: 1e-6, rho0: 0.1,
        }
    }
}

pub const N: usize = 8;
pub const Y0: [f64; N] = [9e5, 1e2, 4.5e4, 5e5, 2e4, 0.0, 0.0, 0.0];

/// Cyclic menstrual influx (eq. 2.9 as coded): 2e4 * sin(pi (t+12)/28)^160 (see vmath).
#[inline]
pub fn mu_e_cyclic(t: f64) -> f64 { vmath::mu_e_cyclic(t) }

/// The three swept parameters per lane; everything else comes from `base`. `LaneParams<f64>` is one cell.
#[derive(Clone, Copy)]
pub struct LaneParams<T: Lane> { pub base: Params, pub omega: T, pub beta1: T, pub rho0: T }

impl From<Params> for LaneParams<f64> {
    fn from(p: Params) -> Self { LaneParams { base: p, omega: p.omega, beta1: p.beta1, rho0: p.rho0 } }
}

/// Reciprocals of the carrying capacities, computed once per integration (the card has no vector divide).
#[derive(Clone, Copy)]
pub struct Inv { pub mc: f64, pub kc: f64, pub ec: f64 }

impl Inv { pub fn of(p: &Params) -> Inv { Inv { mc: 1.0 / p.mc, kc: 1.0 / p.kc, ec: 1.0 / p.ec } } }

/// Right-hand side, written once for `f64` and `f64x8`: the same fused-multiply-add sequence on
/// both, so the scalar and lane solvers are bit-identical. Divisions go through `vmath::div`.
#[inline(always)]
pub fn rhs<T: Lane>(t: T, y: &[T; N], p: &LaneParams<T>, inv: &Inv, const_mu_e: bool, dy: &mut [T; N]) {
    let mu_e = if const_mu_e { T::splat(p.base.mu_e) } else { vmath::mu_e_cyclic(t) };
    rhs_mu(y, p, inv, mu_e, dy)
}

/// `rhs` with the influx supplied by the caller (the lane scheduler mixes constant and cyclic lanes).
#[inline(always)]
pub fn rhs_mu<T: Lane>(y: &[T; N], p: &LaneParams<T>, inv: &Inv, mu_e: T, dy: &mut [T; N]) {
    let b = &p.base;
    let s = T::splat;
    let one = s(1.0);
    let (m0, m1, m2, k0, ka, e0, ef, ea) = (y[0], y[1], y[2], y[3], y[4], y[5], y[6], y[7]);
    let mtot = m0 + m1 + m2;
    let ka_hill = vmath::div(ka, s(b.cka) + ka);
    let m1_hill = vmath::div(m1, s(b.cm1) + m1);
    let m2_hill = vmath::div(m2, s(b.cm2) + m2);
    let e_ect = ef + ea;
    let clear = p.omega * (s(b.gamma) * ka + s(1.0 - b.gamma) * m1);
    // dM0 = mu_m + eta_m (1 - mtot/mc) mtot - beta1 E_ect M0 - theta_m KA_hill M0 - beta2 M0 - delta_m M0
    let logistic_m = mtot.mul_add(s(-inv.mc), one);
    let attach = p.beta1 * e_ect;
    let act = s(b.theta_m) * ka_hill;
    let d0 = (s(b.eta_m) * logistic_m).mul_add(mtot, s(b.mu_m));
    let d0 = (-attach).mul_add(m0, d0);
    let d0 = (-act).mul_add(m0, d0);
    let d0 = s(-b.beta2).mul_add(m0, d0);
    dy[0] = s(-b.delta_m).mul_add(m0, d0);
    // dM1 = beta1 E_ect M0 + theta_m KA_hill M0 + beta21 M2 - beta12 M1 - delta_m M1
    let d1 = act.mul_add(m0, attach * m0);
    let d1 = s(b.beta21).mul_add(m2, d1);
    let d1 = s(-b.beta12).mul_add(m1, d1);
    dy[1] = s(-b.delta_m).mul_add(m1, d1);
    // dM2 = beta2 M0 + beta12 M1 - beta21 M2 - delta_m M2
    let d2 = s(b.beta12).mul_add(m1, s(b.beta2) * m0);
    let d2 = s(-b.beta21).mul_add(m2, d2);
    dy[2] = s(-b.delta_m).mul_add(m2, d2);
    // dK0 = mu_k - theta_k K0 M1_hill - delta_k K0
    let kact = s(b.theta_k) * k0 * m1_hill;
    dy[3] = s(-b.delta_k).mul_add(k0, s(b.mu_k) - kact);
    // dKA = theta_k K0 M1_hill + eta_k (1 - (KA+K0)/kc) KA - sigma E_ect KA - delta_k KA
    let logistic_k = (ka + k0).mul_add(s(-inv.kc), one);
    let d4 = (s(b.eta_k) * logistic_k).mul_add(ka, kact);
    let d4 = (s(-b.sigma) * e_ect).mul_add(ka, d4);
    dy[4] = s(-b.delta_k).mul_add(ka, d4);
    // dE0 = mu_e - delta_e0 (rho0 E0 + (1 - rho0) E0)
    let shed = p.rho0.mul_add(e0, (one - p.rho0) * e0);
    dy[5] = s(-b.delta_e0).mul_add(shed, mu_e);
    // dEF = delta_e0 rho0 E0 - clear EF - rho_f EF - delta_e EF
    let d6 = (-clear).mul_add(ef, s(b.delta_e0) * p.rho0 * e0);
    let d6 = s(-b.rho_f).mul_add(ef, d6);
    dy[6] = s(-b.delta_e).mul_add(ef, d6);
    // dEA = rho_f EF + eta_e (1 - EA/ec) M2_hill EA - clear EA - delta_e EA
    let logistic_e = ea.mul_add(s(-inv.ec), one);
    let d7 = (s(b.eta_e) * logistic_e * m2_hill).mul_add(ea, s(b.rho_f) * ef);
    let d7 = (-clear).mul_add(ea, d7);
    dy[7] = s(-b.delta_e).mul_add(ea, d7);
}

/// Disease-free stability condition (paper eq. 3.1): growth < clearance.
pub fn condition_low_ea(y: &[f64; N], p: &Params) -> (bool, f64, f64) {
    let growth = p.eta_e * y[2] / (p.cm2 + y[2]);
    let clear = p.omega * (p.gamma * y[4] + (1.0 - p.gamma) * y[1]) + p.delta_e;
    (growth < clear, growth, clear)
}
