//! Global sensitivity analysis: Saltelli sampling + Sobol first-order and total indices
//! (Saltelli 2010 / Jansen estimators) over all model parameters, log-uniform within
//! a factor `span` of the defaults. Outputs: log10(EA at the last cycle end) and the
//! disease indicator. No dependencies; its own RNG.

use crate::model::Params;

pub const NAMES: [&str; 27] = [
    "mu_m", "mu_k", "eta_e", "eta_k", "eta_m", "delta_m", "delta_k", "delta_e0", "delta_e", "sigma",
    "mc", "kc", "ec", "cm1", "cm2", "cka", "beta2", "beta12", "beta21", "theta_m", "theta_k",
    "gamma", "rho_f", "mu_e", "omega", "beta1", "rho0",
];

pub fn get(p: &Params) -> [f64; 27] {
    [p.mu_m, p.mu_k, p.eta_e, p.eta_k, p.eta_m, p.delta_m, p.delta_k, p.delta_e0, p.delta_e, p.sigma,
     p.mc, p.kc, p.ec, p.cm1, p.cm2, p.cka, p.beta2, p.beta12, p.beta21, p.theta_m, p.theta_k,
     p.gamma, p.rho_f, p.mu_e, p.omega, p.beta1, p.rho0]
}

pub fn set(v: &[f64; 27]) -> Params {
    Params {
        mu_m: v[0], mu_k: v[1], eta_e: v[2], eta_k: v[3], eta_m: v[4], delta_m: v[5], delta_k: v[6],
        delta_e0: v[7], delta_e: v[8], sigma: v[9], mc: v[10], kc: v[11], ec: v[12], cm1: v[13],
        cm2: v[14], cka: v[15], beta2: v[16], beta12: v[17], beta21: v[18], theta_m: v[19],
        theta_k: v[20], gamma: v[21], rho_f: v[22], mu_e: v[23], omega: v[24], beta1: v[25], rho0: v[26],
    }
}

/// Map a unit-hypercube point to parameters: log-uniform in [default/span, default*span],
/// except fractions (gamma, rho0, rho_f) which are uniform in [default/span, min(1, default*span)].
pub fn map(u: &[f64; 27], span: f64) -> [f64; 27] {
    let d = get(&Params::default());
    let mut v = [0.0; 27];
    for i in 0..27 {
        let is_frac = matches!(NAMES[i], "gamma" | "rho0" | "rho_f");
        if is_frac {
            let lo = d[i] / span; let hi = (d[i] * span).min(1.0);
            v[i] = lo + u[i] * (hi - lo);
        } else {
            let lo = (d[i] / span).ln(); let hi = (d[i] * span).ln();
            v[i] = (lo + u[i] * (hi - lo)).exp();
        }
    }
    v
}

pub struct Indices { pub s1: Vec<f64>, pub st: Vec<f64>, pub var: f64, pub mean: f64 }

/// y_a, y_b: N each; y_ab[i]: N values for matrix A with column i from B.
pub fn sobol(y_a: &[f64], y_b: &[f64], y_ab: &[Vec<f64>]) -> Indices {
    let n = y_a.len() as f64;
    let all: Vec<f64> = y_a.iter().chain(y_b.iter()).cloned().collect();
    let mean = all.iter().sum::<f64>() / all.len() as f64;
    let var = all.iter().map(|y| (y - mean).powi(2)).sum::<f64>() / all.len() as f64;
    let mut s1 = Vec::new(); let mut st = Vec::new();
    for yi in y_ab {
        // Saltelli 2010 first order: V_i = 1/N sum y_b (y_abi - y_a)
        let vi: f64 = (0..y_a.len()).map(|k| y_b[k] * (yi[k] - y_a[k])).sum::<f64>() / n;
        // Jansen total: VT_i = 1/(2N) sum (y_a - y_abi)^2
        let vt: f64 = (0..y_a.len()).map(|k| (y_a[k] - yi[k]).powi(2)).sum::<f64>() / (2.0 * n);
        s1.push(vi / var); st.push(vt / var);
    }
    Indices { s1, st, var, mean }
}
