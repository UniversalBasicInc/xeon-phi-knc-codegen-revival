//! Dormand–Prince RK5(4) with adaptive steps and dense sampling at fixed output times.
//! Tolerances chosen to match the authors' ode15s settings (RelTol = AbsTol = 1e-6).
//! Arithmetic goes through `vmath` (fused chains, reciprocal division, polynomial pow) so that
//! this scalar solver and the eight-lane solver in `simd.rs` execute the same operation sequence.

use crate::model::{rhs, Inv, LaneParams, Params, N};
use crate::stiff;
use crate::vmath::{self, comb};

pub struct Solver { pub rtol: f64, pub atol: f64, pub h_init: f64, pub h_max: f64 }

impl Default for Solver {
    /// ENDOMC_TOL overrides rtol = atol (default 1e-6, the authors' ode15s setting).
    fn default() -> Self {
        let tol = std::env::var("ENDOMC_TOL").ok().and_then(|v| v.parse().ok()).unwrap_or(1e-6);
        Solver { rtol: tol, atol: tol, h_init: 1e-3, h_max: 0.5 }
    }
}

pub(crate) const A21: f64 = 1.0 / 5.0;
pub(crate) const A31: f64 = 3.0 / 40.0; pub(crate) const A32: f64 = 9.0 / 40.0;
pub(crate) const A41: f64 = 44.0 / 45.0; pub(crate) const A42: f64 = -56.0 / 15.0; pub(crate) const A43: f64 = 32.0 / 9.0;
pub(crate) const A51: f64 = 19372.0 / 6561.0; pub(crate) const A52: f64 = -25360.0 / 2187.0; pub(crate) const A53: f64 = 64448.0 / 6561.0; pub(crate) const A54: f64 = -212.0 / 729.0;
pub(crate) const A61: f64 = 9017.0 / 3168.0; pub(crate) const A62: f64 = -355.0 / 33.0; pub(crate) const A63: f64 = 46732.0 / 5247.0; pub(crate) const A64: f64 = 49.0 / 176.0; pub(crate) const A65: f64 = -5103.0 / 18656.0;
pub(crate) const B1: f64 = 35.0 / 384.0; pub(crate) const B3: f64 = 500.0 / 1113.0; pub(crate) const B4: f64 = 125.0 / 192.0; pub(crate) const B5: f64 = -2187.0 / 6784.0; pub(crate) const B6: f64 = 11.0 / 84.0;
pub(crate) const E1: f64 = 71.0 / 57600.0; pub(crate) const E3: f64 = -71.0 / 16695.0; pub(crate) const E4: f64 = 71.0 / 1920.0; pub(crate) const E5: f64 = -17253.0 / 339200.0; pub(crate) const E6: f64 = 22.0 / 525.0; pub(crate) const E7: f64 = -1.0 / 40.0;
/// stage times as multipliers of h (no divisions on the card)
pub(crate) const C2: f64 = 0.2; pub(crate) const C3: f64 = 0.3; pub(crate) const C4: f64 = 0.8; pub(crate) const C5: f64 = 8.0 / 9.0;

impl Solver {
    /// Integrate from t0 with state y0; call `out(t, y)` at every t in `times` (ascending).
    /// Returns the state at the last output time and the number of RHS evaluations.
    pub fn integrate<F: FnMut(f64, &[f64; N])>(
        &self, p: &Params, const_mu_e: bool, t0: f64, y0: [f64; N], times: &[f64], mut out: F,
    ) -> ([f64; N], u64) {
        let lp: LaneParams<f64> = (*p).into();
        let inv = Inv::of(p);
        let mut t = t0;
        let mut y = y0;
        let mut h = self.h_init;
        let mut nfev: u64 = 0;
        let (mut k1, mut k2, mut k3, mut k4, mut k5, mut k6, mut k7) = ([0.0; N], [0.0; N], [0.0; N], [0.0; N], [0.0; N], [0.0; N], [0.0; N]);
        let mut ytmp = [0.0; N];
        rhs(t, &y, &lp, &inv, const_mu_e, &mut k1); nfev += 1;
        for &t_out in times {
            while t < t_out {
                let mut hh = h.min(t_out - t).min(self.h_max);
                loop {
                    for i in 0..N { ytmp[i] = comb(y[i], hh, [A21], [k1[i]]); }
                    rhs(t + hh * C2, &ytmp, &lp, &inv, const_mu_e, &mut k2);
                    for i in 0..N { ytmp[i] = comb(y[i], hh, [A31, A32], [k1[i], k2[i]]); }
                    rhs(t + hh * C3, &ytmp, &lp, &inv, const_mu_e, &mut k3);
                    for i in 0..N { ytmp[i] = comb(y[i], hh, [A41, A42, A43], [k1[i], k2[i], k3[i]]); }
                    rhs(t + hh * C4, &ytmp, &lp, &inv, const_mu_e, &mut k4);
                    for i in 0..N { ytmp[i] = comb(y[i], hh, [A51, A52, A53, A54], [k1[i], k2[i], k3[i], k4[i]]); }
                    rhs(t + hh * C5, &ytmp, &lp, &inv, const_mu_e, &mut k5);
                    for i in 0..N { ytmp[i] = comb(y[i], hh, [A61, A62, A63, A64, A65], [k1[i], k2[i], k3[i], k4[i], k5[i]]); }
                    rhs(t + hh, &ytmp, &lp, &inv, const_mu_e, &mut k6);
                    let mut ynew = [0.0; N];
                    for i in 0..N { ynew[i] = comb(y[i], hh, [B1, B3, B4, B5, B6], [k1[i], k3[i], k4[i], k5[i], k6[i]]); }
                    rhs(t + hh, &ynew, &lp, &inv, const_mu_e, &mut k7);
                    nfev += 6;
                    let mut err = 0.0f64;
                    for i in 0..N {
                        let e = comb(0.0, hh, [E1, E3, E4, E5, E6, E7], [k1[i], k3[i], k4[i], k5[i], k6[i], k7[i]]);
                        let sc = self.rtol.mul_add(y[i].abs().max(ynew[i].abs()), self.atol);
                        err = err.max(vmath::div(e, sc).abs());
                    }
                    if err <= 1.0 {
                        t += hh;
                        y = ynew;
                        k1 = k7;
                        let fac = (0.9 * vmath::powf(err.max(1e-10), -0.2)).min(5.0).max(0.2);
                        h = (hh * fac).min(self.h_max);
                        break;
                    } else {
                        let fac = (0.9 * vmath::powf(err, -0.25)).max(0.1);
                        hh *= fac;
                        if hh < 1e-12 { panic!("step size underflow at t={t}"); }
                    }
                }
            }
            out(t, &y);
        }
        (y, nfev)
    }

    /// Same contract as `integrate`, with the ode23s Rosenbrock step (stiff.rs).
    pub fn integrate_stiff<F: FnMut(f64, &[f64; N])>(
        &self, p: &Params, const_mu_e: bool, t0: f64, y0: [f64; N], times: &[f64], mut out: F,
    ) -> ([f64; N], u64) {
        let lp: LaneParams<f64> = (*p).into();
        let inv = Inv::of(p);
        let mu_c = p.mu_e;
        let mu = move |t: f64| if const_mu_e { mu_c } else { vmath::mu_e_cyclic(t) };
        let dmu = move |t: f64| if const_mu_e { 0.0 } else { vmath::mu_e_cyclic_dt(t) };
        let mut t = t0;
        let mut y = y0;
        let mut h = self.h_init;
        let mut nfev: u64 = 0;
        let mut f0 = [0.0; N];
        rhs(t, &y, &lp, &inv, const_mu_e, &mut f0); nfev += 1;
        for &t_out in times {
            while t < t_out {
                let mut hh = h.min(t_out - t).min(self.h_max);
                loop {
                    let (ynew, fnew, err) = stiff::step(&lp, &inv, t, &y, hh, &f0, self.rtol, self.atol, &mu, &dmu);
                    nfev += 2;
                    if err <= 1.0 {
                        t += hh;
                        y = ynew;
                        f0 = fnew;
                        let fac = (0.9 * vmath::powf(err.max(1e-10), stiff::EXP_ACC)).min(5.0).max(0.2);
                        h = (hh * fac).min(self.h_max);
                        break;
                    } else {
                        let fac = (0.9 * vmath::powf(err, stiff::EXP_REJ)).max(0.1);
                        hh *= fac;
                        if hh < 1e-12 { panic!("step size underflow at t={t}"); }
                    }
                }
            }
            out(t, &y);
        }
        (y, nfev)
    }
}
