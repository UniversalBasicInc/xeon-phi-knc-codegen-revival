//! Photon interaction data for water (NIST XCOM, cm²/g) and electron CSDA ranges in water
//! (NIST ESTAR, g/cm²). Lung is modelled as water-equivalent composition at low density
//! (ICRU lung, 0.26 g/cm³), which is the standard first approximation for slab phantoms.
//!
//! ⚠ Table values must be re-checked against NIST XCOM / ESTAR before any validation claim.
//! Energies in MeV. Columns: E, photoelectric, incoherent (Compton), pair (nuclear+electron).

pub const ME: f64 = 0.510_998_95; // electron rest energy, MeV

// E, tau (photoelectric), sigma_c (incoherent), kappa (pair) — water, cm²/g
pub const WATER_XCOM: [[f64; 4]; 16] = [
    [0.010, 4.944e+0, 1.550e-1, 0.0],
    [0.015, 1.374e+0, 1.699e-1, 0.0],
    [0.020, 5.437e-1, 1.774e-1, 0.0],
    [0.030, 1.478e-1, 1.818e-1, 0.0],
    [0.040, 5.830e-2, 1.814e-1, 0.0],
    [0.050, 2.823e-2, 1.790e-1, 0.0],
    [0.060, 1.580e-2, 1.754e-1, 0.0],
    [0.080, 6.207e-3, 1.675e-1, 0.0],
    [0.100, 3.011e-3, 1.605e-1, 0.0],
    [0.150, 8.320e-4, 1.448e-1, 0.0],
    [0.200, 3.496e-4, 1.335e-1, 0.0],
    [0.300, 1.001e-4, 1.179e-1, 0.0],
    [0.500, 2.660e-5, 9.660e-2, 0.0],
    [1.000, 7.230e-6, 7.066e-2, 0.0],
    [1.500, 3.880e-6, 5.750e-2, 2.500e-4],
    [6.000, 6.200e-7, 2.190e-2, 5.700e-3],
];

// E (MeV), CSDA range in water (g/cm²) — NIST ESTAR
pub const WATER_ESTAR: [[f64; 2]; 14] = [
    [0.010, 2.515e-4],
    [0.020, 8.566e-4],
    [0.050, 4.320e-3],
    [0.100, 1.431e-2],
    [0.200, 4.488e-2],
    [0.300, 8.421e-2],
    [0.500, 1.766e-1],
    [0.700, 2.769e-1],
    [1.000, 4.367e-1],
    [1.500, 7.075e-1],
    [2.000, 9.785e-1],
    [3.000, 1.514e+0],
    [5.000, 2.550e+0],
    [10.000, 4.975e+0],
];

#[inline]
fn log_interp(x: f64, xs: impl Fn(usize) -> f64, ys: impl Fn(usize) -> f64, n: usize) -> f64 {
    if x <= xs(0) {
        return ys(0);
    }
    if x >= xs(n - 1) {
        return ys(n - 1);
    }
    let mut i = 0;
    while xs(i + 1) < x {
        i += 1;
    }
    let (x0, x1, y0, y1) = (xs(i), xs(i + 1), ys(i), ys(i + 1));
    if y0 <= 0.0 || y1 <= 0.0 {
        let t = (x - x0) / (x1 - x0);
        return y0 + t * (y1 - y0);
    }
    let t = (x.ln() - x0.ln()) / (x1.ln() - x0.ln());
    (y0.ln() + t * (y1.ln() - y0.ln())).exp()
}

/// Mass attenuation coefficients (cm²/g) of water at energy e: (photoelectric, compton, pair).
pub fn water_mu_rho(e: f64) -> (f64, f64, f64) {
    let n = WATER_XCOM.len();
    (
        log_interp(e, |i| WATER_XCOM[i][0], |i| WATER_XCOM[i][1], n),
        log_interp(e, |i| WATER_XCOM[i][0], |i| WATER_XCOM[i][2], n),
        log_interp(e, |i| WATER_XCOM[i][0], |i| WATER_XCOM[i][3], n),
    )
}

/// CSDA range in water (g/cm²) for an electron of kinetic energy t.
pub fn water_csda_range(t: f64) -> f64 {
    let n = WATER_ESTAR.len();
    log_interp(t, |i| WATER_ESTAR[i][0], |i| WATER_ESTAR[i][1], n)
}
