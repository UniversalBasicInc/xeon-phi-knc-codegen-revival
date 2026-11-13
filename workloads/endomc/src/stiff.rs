//! Linearly implicit stiff solver: the modified Rosenbrock pair of MATLAB's ode23s
//! (Shampine & Reichelt 1997, "The MATLAB ODE Suite", §4), order 2 with an order-3 error estimate.
//! The authors solved this model with ode15s; the low-ω cells are stiff enough that the explicit
//! Dormand–Prince solver needs ~1e7 steps there. One Rosenbrock step = analytic Jacobian, one LU of
//! W = I - h d J (8×8, no pivoting: W is diagonally dominant for the step sizes the controller
//! chooses), three rhs evaluations and three triangular solves. Written once over `Lane`, so the
//! scalar and eight-lane drivers execute the same operations and stay bit-identical.

use crate::model::{rhs_mu, Inv, LaneParams, N};
use crate::vmath::{self, Lane};

pub const D: f64 = 0.292_893_218_813_452_5;      // 1/(2+sqrt 2)
pub const E32: f64 = 7.414_213_562_373_095;      // 6 + sqrt 2
/// step controller exponents (error estimate is order 3)
pub const EXP_ACC: f64 = -1.0 / 3.0;
pub const EXP_REJ: f64 = -1.0 / 3.0;

/// Analytic Jacobian of the right-hand side (∂f_i/∂y_j), independent of t.
#[inline(always)]
pub fn jacobian<T: Lane>(y: &[T; N], p: &LaneParams<T>, inv: &Inv, j: &mut [[T; N]; N]) {
    let b = &p.base;
    let s = T::splat;
    let one = s(1.0);
    let zero = s(0.0);
    let (m0, m1, m2, k0, ka, e0, ef, ea) = (y[0], y[1], y[2], y[3], y[4], y[5], y[6], y[7]);
    let mtot = m0 + m1 + m2;
    let rka = vmath::rcp(s(b.cka) + ka);
    let rm1 = vmath::rcp(s(b.cm1) + m1);
    let rm2 = vmath::rcp(s(b.cm2) + m2);
    let ka_hill = ka * rka;            let dka_hill = s(b.cka) * rka * rka;
    let m1_hill = m1 * rm1;            let dm1_hill = s(b.cm1) * rm1 * rm1;
    let m2_hill = m2 * rm2;            let dm2_hill = s(b.cm2) * rm2 * rm2;
    let e_ect = ef + ea;
    let clear = p.omega * (s(b.gamma) * ka + s(1.0 - b.gamma) * m1);
    let dclear_dka = p.omega * s(b.gamma);
    let dclear_dm1 = p.omega * s(1.0 - b.gamma);
    for r in j.iter_mut() { for v in r.iter_mut() { *v = zero; } }
    // f0
    let gm = s(b.eta_m) * mtot.mul_add(s(-2.0 * inv.mc), one);           // eta_m (1 - 2 mtot/mc)
    j[0][0] = gm - p.beta1 * e_ect - s(b.theta_m) * ka_hill - s(b.beta2) - s(b.delta_m);
    j[0][1] = gm; j[0][2] = gm;
    j[0][4] = -(s(b.theta_m) * dka_hill * m0);
    j[0][6] = -(p.beta1 * m0); j[0][7] = j[0][6];
    // f1
    j[1][0] = p.beta1 * e_ect + s(b.theta_m) * ka_hill;
    j[1][1] = s(-b.beta12 - b.delta_m);
    j[1][2] = s(b.beta21);
    j[1][4] = s(b.theta_m) * dka_hill * m0;
    j[1][6] = p.beta1 * m0; j[1][7] = j[1][6];
    // f2
    j[2][0] = s(b.beta2); j[2][1] = s(b.beta12); j[2][2] = s(-b.beta21 - b.delta_m);
    // f3
    j[3][1] = -(s(b.theta_k) * k0 * dm1_hill);
    j[3][3] = -(s(b.theta_k) * m1_hill) - s(b.delta_k);
    // f4
    j[4][1] = s(b.theta_k) * k0 * dm1_hill;
    j[4][3] = s(b.theta_k) * m1_hill - s(b.eta_k * inv.kc) * ka;
    j[4][4] = s(b.eta_k) * (ka + k0).mul_add(s(-inv.kc), one) - s(b.eta_k * inv.kc) * ka - s(b.sigma) * e_ect - s(b.delta_k);
    j[4][6] = -(s(b.sigma) * ka); j[4][7] = j[4][6];
    // f5: mu_e - delta_e0 (rho0 e0 + (1 - rho0) e0)
    j[5][5] = -(s(b.delta_e0) * (p.rho0 + (one - p.rho0)));
    // f6
    j[6][1] = -(dclear_dm1 * ef);
    j[6][4] = -(dclear_dka * ef);
    j[6][5] = s(b.delta_e0) * p.rho0;
    j[6][6] = -clear - s(b.rho_f) - s(b.delta_e);
    // f7
    j[7][1] = -(dclear_dm1 * ea);
    j[7][2] = s(b.eta_e) * ea.mul_add(s(-inv.ec), one) * ea * dm2_hill;
    j[7][4] = -(dclear_dka * ea);
    j[7][6] = s(b.rho_f);
    j[7][7] = s(b.eta_e) * m2_hill * ea.mul_add(s(-2.0 * inv.ec), one) - clear - s(b.delta_e);
}

/// In-place LU without pivoting; returns the reciprocals of the pivots.
#[inline(always)]
pub fn lu<T: Lane>(a: &mut [[T; N]; N]) -> [T; N] {
    let mut rp = [T::splat(0.0); N];
    for k in 0..N {
        rp[k] = vmath::rcp(a[k][k]);
        for i in (k + 1)..N {
            let f = a[i][k] * rp[k];
            a[i][k] = f;
            for c in (k + 1)..N { a[i][c] = (-f).mul_add(a[k][c], a[i][c]); }
        }
    }
    rp
}

/// Solve (LU) x = b.
#[inline(always)]
pub fn lu_solve<T: Lane>(a: &[[T; N]; N], rp: &[T; N], b: &[T; N]) -> [T; N] {
    let mut x = *b;
    for i in 1..N { for c in 0..i { x[i] = (-a[i][c]).mul_add(x[c], x[i]); } }
    for i in (0..N).rev() {
        for c in (i + 1)..N { x[i] = (-a[i][c]).mul_add(x[c], x[i]); }
        x[i] = x[i] * rp[i];
    }
    x
}

/// One ode23s step from (t, y) with f0 = f(t, y). `mu(t)` and `dmu(t)` give the influx and its time
/// derivative (zero in the constant stage). Returns (y_new, f(t+h, y_new), err).
#[inline(always)]
pub fn step<T: Lane>(p: &LaneParams<T>, inv: &Inv, t: T, y: &[T; N], hh: T, f0: &[T; N], rtol: f64, atol: f64,
                     mu: &impl Fn(T) -> T, dmu: &impl Fn(T) -> T) -> ([T; N], [T; N], T) {
    let s = T::splat;
    let hd = hh * s(D);
    let mut w = [[s(0.0); N]; N];
    jacobian(y, p, inv, &mut w);
    for i in 0..N { for c in 0..N { w[i][c] = (-hd) * w[i][c]; } w[i][i] = w[i][i] + s(1.0); }
    let rp = lu(&mut w);
    // h d T: only the E0 equation depends on t
    let mut hdt = [s(0.0); N];
    hdt[5] = hd * dmu(t);
    let mut r1 = [s(0.0); N];
    for i in 0..N { r1[i] = f0[i] + hdt[i]; }
    let k1 = lu_solve(&w, &rp, &r1);
    let half = hh * s(0.5);
    let mut ytmp = [s(0.0); N];
    for i in 0..N { ytmp[i] = half.mul_add(k1[i], y[i]); }
    let t_half = t + half;
    let mut f1 = [s(0.0); N];
    rhs_mu(&ytmp, p, inv, mu(t_half), &mut f1);
    let mut r2 = [s(0.0); N];
    for i in 0..N { r2[i] = f1[i] - k1[i]; }
    let k2s = lu_solve(&w, &rp, &r2);
    let mut k2 = [s(0.0); N];
    for i in 0..N { k2[i] = k2s[i] + k1[i]; }
    let mut ynew = [s(0.0); N];
    for i in 0..N { ynew[i] = hh.mul_add(k2[i], y[i]); }
    let t_new = t + hh;
    let mut f2 = [s(0.0); N];
    rhs_mu(&ynew, p, inv, mu(t_new), &mut f2);
    let mut r3 = [s(0.0); N];
    for i in 0..N { r3[i] = f2[i] - s(E32) * (k2[i] - f1[i]) - s(2.0) * (k1[i] - f0[i]) + hdt[i]; }
    let k3 = lu_solve(&w, &rp, &r3);
    let h6 = hh * s(1.0 / 6.0);
    let mut err = s(0.0);
    for i in 0..N {
        let e = h6 * (k1[i] - s(2.0) * k2[i] + k3[i]);
        let sc = s(rtol).mul_add(y[i].abs().max(ynew[i].abs()), s(atol));
        err = err.max(vmath::div(e, sc).abs());
    }
    (ynew, f2, err)
}
