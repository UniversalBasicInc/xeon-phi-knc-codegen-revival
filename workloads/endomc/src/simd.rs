//! Two lane solvers. (1) `solve_two_stage_lanes`: eight cells advance together through both
//! stages (the default sweep path). (2) `run_jobs`, a per-lane refill scheduler: measured 27 % slower
//! on the card because mixed-stage lanes pay the cyclic influx polynomial on every step, kept for
//! reference (`sweep-simd-refill`).
//!
//! Lane scheduler: eight independent cells advance together in one `f64x8` vector, each lane
//! keeping its own job, stage (constant influx → median → cyclic), time, step size and output
//! pointer. When a lane finishes a cell it immediately takes the next one from the queue, so no
//! lane ever waits on its seven siblings. Same Dormand–Prince RK5(4) coefficients, tolerances and
//! step controller as `ode.rs`, applied per lane with masks, so each lane follows exactly the
//! sequence of accepted and rejected steps the scalar solver would, and every operation (via
//! `vmath`) lowers to native Xeon Phi (KNC) 512-bit instructions.

use crate::model::{rhs, rhs_mu, Inv, Params, N, Y0};
use crate::ode::{A21, A31, A32, A41, A42, A43, A51, A52, A53, A54, A61, A62, A63, A64, A65, B1, B3, B4, B5, B6, C2, C3, C4, C5, E1, E3, E4, E5, E6, E7};
use crate::stiff;
use crate::vmath::{self, comb};
use std::simd::{prelude::*, f64x8, Mask, StdFloat};

pub const L: usize = 8;
type V = f64x8;
type M = Mask<i64, L>;
pub type LaneParams = crate::model::LaneParams<V>;

/// One cell of the sweep: the three varied parameters and the caller's id for it.
#[derive(Clone, Copy)]
pub struct Job { pub id: usize, pub rho0: f64, pub omega: f64, pub beta1: f64 }

/// Finished cell: the cyclic-stage EA series, the final state and the number of solver steps.
pub struct Done { pub id: usize, pub ea: Vec<(f64, f64)>, pub last: [f64; N], pub steps: u64 }

#[derive(Clone, Copy, PartialEq)]
enum Stage { Const, Cyclic, Idle }

/// Run cells from `next_job` (called whenever a lane is free; `None` = queue empty) until all are
/// done, calling `on_done` for each. `t_end_cyclic`, `dt`, `t_min_eval` as in `solve_two_stage`.
pub fn run_jobs(base: &Params, t_end_cyclic: f64, dt: f64, t_min_eval: f64, rtol: f64, atol: f64, h_init: f64, h_max: f64,
                mut next_job: impl FnMut() -> Option<Job>, mut on_done: impl FnMut(Done)) {
    let s = V::splat;
    let inv = Inv::of(base);
    let n_const = (5000.0 / dt) as usize;
    let times_const: Vec<f64> = (1..=n_const).map(|i| i as f64 * dt).collect();
    let n_cyc = (t_end_cyclic / dt) as usize;
    let times_cyc: Vec<f64> = (1..=n_cyc).map(|i| i as f64 * dt).collect();
    let y0_const: [V; N] = std::array::from_fn(|i| s(Y0[i]));

    let mut p = LaneParams { base: *base, omega: s(base.omega), beta1: s(base.beta1), rho0: s(base.rho0) };
    let mut stage = [Stage::Idle; L];
    let mut job = [Job { id: 0, rho0: 0.0, omega: 0.0, beta1: 0.0 }; L];
    let mut idx = [0usize; L];
    let mut samples: Vec<Vec<[f64; N]>> = (0..L).map(|_| Vec::with_capacity(n_const)).collect();
    let mut ea: Vec<Vec<(f64, f64)>> = (0..L).map(|_| Vec::with_capacity(n_cyc)).collect();
    let mut t = s(0.0);
    let mut y = y0_const;
    let mut h = s(h_init);
    let mut hh = s(0.0);
    let mut need_new_hh = M::splat(true);
    let mut active = M::splat(false);
    let mut const_lanes = M::splat(false);
    let mut steps = s(0.0);
    let z = [s(0.0); N];
    let (mut k1, mut k2, mut k3, mut k4, mut k5, mut k6, mut k7) = (z, z, z, z, z, z, z);
    let mut ytmp = z;
    let mut queue_open = true;

    // (re)start a lane on a fresh cell at t = 0 with the constant-influx stage
    let mut start_lane = |l: usize, j: Job, p: &mut LaneParams, stage: &mut [Stage; L], job: &mut [Job; L], idx: &mut [usize; L], t: &mut V, y: &mut [V; N], h: &mut V, active: &mut M, const_lanes: &mut M, need_new_hh: &mut M, steps: &mut V| {
        let m = M::from_bitmask(1 << l);
        p.omega = m.select(s(j.omega), p.omega);
        p.beta1 = m.select(s(j.beta1), p.beta1);
        p.rho0 = m.select(s(j.rho0), p.rho0);
        stage[l] = Stage::Const; job[l] = j; idx[l] = 0;
        *t = m.select(s(0.0), *t);
        for i in 0..N { y[i] = m.select(s(Y0[i]), y[i]); }
        *h = m.select(s(h_init), *h);
        *steps = m.select(s(0.0), *steps);
        active.set(l, true); const_lanes.set(l, true); need_new_hh.set(l, true);
    };
    for l in 0..L {
        match next_job() {
            Some(j) => start_lane(l, j, &mut p, &mut stage, &mut job, &mut idx, &mut t, &mut y, &mut h, &mut active, &mut const_lanes, &mut need_new_hh, &mut steps),
            None => { queue_open = false; break; }
        }
    }
    let t_out_of = |stage: &[Stage; L], idx: &[usize; L]| V::from_array(std::array::from_fn(|l| match stage[l] {
        Stage::Const => times_const[idx[l]],
        Stage::Cyclic => times_cyc[idx[l]],
        Stage::Idle => f64::INFINITY,
    }));
    let mut t_out = t_out_of(&stage, &idx);
    let mu_const = s(base.mu_e);
    // k1 = rhs(t0, y0) for every lane that just (re)started: computed for all lanes, kept for the new ones
    let mut k1_stale = M::splat(true);

    while active.any() {
        if k1_stale.any() {
            let mu = const_lanes.select(mu_const, vmath::mu_e_cyclic(t));
            rhs_mu(&y, &p, &inv, mu, &mut k2);
            for i in 0..N { k1[i] = k1_stale.select(k2[i], k1[i]); }
            k1_stale = M::splat(false);
        }
        let fresh = h.simd_min(t_out - t).simd_min(s(h_max));
        hh = need_new_hh.select(fresh, hh);
        hh = active.select(hh, s(0.0));
        for i in 0..N { ytmp[i] = comb(y[i], hh, [A21], [k1[i]]); }
        let ts = t + hh * s(C2); rhs_mu(&ytmp, &p, &inv, const_lanes.select(mu_const, vmath::mu_e_cyclic(ts)), &mut k2);
        for i in 0..N { ytmp[i] = comb(y[i], hh, [A31, A32], [k1[i], k2[i]]); }
        let ts = t + hh * s(C3); rhs_mu(&ytmp, &p, &inv, const_lanes.select(mu_const, vmath::mu_e_cyclic(ts)), &mut k3);
        for i in 0..N { ytmp[i] = comb(y[i], hh, [A41, A42, A43], [k1[i], k2[i], k3[i]]); }
        let ts = t + hh * s(C4); rhs_mu(&ytmp, &p, &inv, const_lanes.select(mu_const, vmath::mu_e_cyclic(ts)), &mut k4);
        for i in 0..N { ytmp[i] = comb(y[i], hh, [A51, A52, A53, A54], [k1[i], k2[i], k3[i], k4[i]]); }
        let ts = t + hh * s(C5); rhs_mu(&ytmp, &p, &inv, const_lanes.select(mu_const, vmath::mu_e_cyclic(ts)), &mut k5);
        for i in 0..N { ytmp[i] = comb(y[i], hh, [A61, A62, A63, A64, A65], [k1[i], k2[i], k3[i], k4[i], k5[i]]); }
        let ts = t + hh; let mu_end = const_lanes.select(mu_const, vmath::mu_e_cyclic(ts));
        rhs_mu(&ytmp, &p, &inv, mu_end, &mut k6);
        let mut ynew = z;
        for i in 0..N { ynew[i] = comb(y[i], hh, [B1, B3, B4, B5, B6], [k1[i], k3[i], k4[i], k5[i], k6[i]]); }
        rhs_mu(&ynew, &p, &inv, mu_end, &mut k7);
        let mut err = s(0.0);
        for i in 0..N {
            let e = comb(s(0.0), hh, [E1, E3, E4, E5, E6, E7], [k1[i], k3[i], k4[i], k5[i], k6[i], k7[i]]);
            let sc = StdFloat::mul_add(s(rtol), y[i].abs().simd_max(ynew[i].abs()), s(atol));
            err = err.simd_max(vmath::div(e, sc).abs());
        }
        let accept = err.simd_le(s(1.0)) & active;
        let pw = vmath::powf(accept.select(err.simd_max(s(1e-10)), err), accept.select(s(-0.2), s(-0.25)));
        let t_acc = t + hh;
        let fac_acc = (s(0.9) * pw).simd_min(s(5.0)).simd_max(s(0.2));
        let h_acc = (hh * fac_acc).simd_min(s(h_max));
        let fac_rej = (s(0.9) * pw).simd_max(s(0.1));
        let hh_rej = hh * fac_rej;
        let reject = !accept & active;
        if (hh_rej.simd_lt(s(1e-12)) & reject).any() {
            let ta = t.to_array();
            let lane = (0..L).find(|&l| reject.test(l) && hh_rej[l] < 1e-12).unwrap();
            panic!("step size underflow at t={} (lane {lane}, job {})", ta[lane], job[lane].id);
        }
        steps += active.select(s(1.0), s(0.0));
        t = accept.select(t_acc, t);
        for i in 0..N { y[i] = accept.select(ynew[i], y[i]); k1[i] = accept.select(k7[i], k1[i]); }
        h = accept.select(h_acc, h);
        hh = reject.select(hh_rej, hh);
        need_new_hh = accept;
        let reached = t.simd_ge(t_out) & accept;
        if reached.any() {
            let ta = t.to_array();
            let ya: [[f64; L]; N] = std::array::from_fn(|i| y[i].to_array());
            let sa = steps.to_array();
            let mut changed = false;
            for l in 0..L {
                if !reached.test(l) { continue; }
                let yl: [f64; N] = std::array::from_fn(|i| ya[i][l]);
                match stage[l] {
                    Stage::Const => {
                        if ta[l] >= t_min_eval { samples[l].push(yl); }
                        idx[l] += 1;
                        if idx[l] >= times_const.len() {
                            // median of the sampled states → cyclic initial condition (as solve_two_stage)
                            let m = M::from_bitmask(1 << l);
                            for i in 0..N {
                                let mut col: Vec<f64> = samples[l].iter().map(|y| y[i]).collect();
                                col.sort_by(|a, b| a.partial_cmp(b).unwrap());
                                let n = col.len();
                                let med = if n % 2 == 1 { col[n / 2] } else { 0.5 * (col[n / 2 - 1] + col[n / 2]) };
                                y[i] = m.select(s(med), y[i]);
                            }
                            samples[l].clear();
                            stage[l] = Stage::Cyclic; idx[l] = 0;
                            t = m.select(s(0.0), t);
                            h = m.select(s(h_init), h);
                            const_lanes.set(l, false); need_new_hh.set(l, true); k1_stale.set(l, true);
                            changed = true;
                        }
                    }
                    Stage::Cyclic => {
                        ea[l].push((ta[l], yl[7]));
                        idx[l] += 1;
                        if idx[l] >= times_cyc.len() {
                            on_done(Done { id: job[l].id, ea: std::mem::replace(&mut ea[l], Vec::with_capacity(n_cyc)), last: yl, steps: sa[l] as u64 });
                            let next = if queue_open { next_job() } else { None };
                            match next {
                                Some(j) => { start_lane(l, j, &mut p, &mut stage, &mut job, &mut idx, &mut t, &mut y, &mut h, &mut active, &mut const_lanes, &mut need_new_hh, &mut steps); k1_stale.set(l, true); }
                                None => { queue_open = false; stage[l] = Stage::Idle; active.set(l, false); }
                            }
                            changed = true;
                        }
                    }
                    Stage::Idle => unreachable!(),
                }
            }
            let _ = changed;
            t_out = t_out_of(&stage, &idx);
        }
    }
}

/// Integrate all eight lanes from `t0` over the shared output grid `times`; `out(lane, t, y)` is
/// called for each lane as it reaches each grid point, in the lane's own order. Returns final states.
pub fn integrate_lanes(p: &LaneParams, const_mu_e: bool, t0: f64, y0: [V; N], times: &[f64], rtol: f64, atol: f64, h_init: f64, h_max: f64, steps: &mut V, mut out: impl FnMut(usize, f64, &[f64; N])) -> ([V; N], u64) {
    let s = V::splat;
    let inv = Inv::of(&p.base);
    let mut t = s(t0);
    let mut y = y0;
    let mut h = s(h_init);
    let mut hh = s(0.0);
    let mut need_new_hh = M::splat(true); // lanes that start a fresh step (accepted or at an output point)
    let mut idx = [0usize; L];               // next output index per lane
    let mut active = M::splat(true);
    let mut nfev: u64 = 0;
    let z = [s(0.0); N];
    let (mut k1, mut k2, mut k3, mut k4, mut k5, mut k6, mut k7) = (z, z, z, z, z, z, z);
    let mut ytmp = z;
    rhs(t, &y, p, &inv, const_mu_e, &mut k1); nfev += 1;
    let t_out_of = |idx: &[usize; L]| V::from_array(std::array::from_fn(|l| if idx[l] < times.len() { times[idx[l]] } else { f64::INFINITY }));
    let mut t_out = t_out_of(&idx);
    while active.any() {
        // fresh step size for lanes that just accepted (or just emitted an output point)
        let fresh = h.simd_min(t_out - t).simd_min(s(h_max));
        hh = need_new_hh.select(fresh, hh);
        hh = active.select(hh, s(0.0));
        for i in 0..N { ytmp[i] = comb(y[i], hh, [A21], [k1[i]]); }
        rhs(t + hh * s(C2), &ytmp, p, &inv, const_mu_e, &mut k2);
        for i in 0..N { ytmp[i] = comb(y[i], hh, [A31, A32], [k1[i], k2[i]]); }
        rhs(t + hh * s(C3), &ytmp, p, &inv, const_mu_e, &mut k3);
        for i in 0..N { ytmp[i] = comb(y[i], hh, [A41, A42, A43], [k1[i], k2[i], k3[i]]); }
        rhs(t + hh * s(C4), &ytmp, p, &inv, const_mu_e, &mut k4);
        for i in 0..N { ytmp[i] = comb(y[i], hh, [A51, A52, A53, A54], [k1[i], k2[i], k3[i], k4[i]]); }
        rhs(t + hh * s(C5), &ytmp, p, &inv, const_mu_e, &mut k5);
        for i in 0..N { ytmp[i] = comb(y[i], hh, [A61, A62, A63, A64, A65], [k1[i], k2[i], k3[i], k4[i], k5[i]]); }
        rhs(t + hh, &ytmp, p, &inv, const_mu_e, &mut k6);
        let mut ynew = z;
        for i in 0..N { ynew[i] = comb(y[i], hh, [B1, B3, B4, B5, B6], [k1[i], k3[i], k4[i], k5[i], k6[i]]); }
        rhs(t + hh, &ynew, p, &inv, const_mu_e, &mut k7);
        nfev += 6;
        let mut err = s(0.0);
        for i in 0..N {
            let e = comb(s(0.0), hh, [E1, E3, E4, E5, E6, E7], [k1[i], k3[i], k4[i], k5[i], k6[i], k7[i]]);
            let sc = StdFloat::mul_add(s(rtol), y[i].abs().simd_max(ynew[i].abs()), s(atol));
            err = err.simd_max(vmath::div(e, sc).abs());
        }
        let accept = err.simd_le(s(1.0)) & active;
        // one vector pow for all lanes: accepted lanes max(err,1e-10)^-0.2, rejected lanes err^-0.25
        let pw = vmath::powf(accept.select(err.simd_max(s(1e-10)), err), accept.select(s(-0.2), s(-0.25)));
        // accepted lanes
        let t_acc = t + hh;
        let fac_acc = (s(0.9) * pw).simd_min(s(5.0)).simd_max(s(0.2));
        let h_acc = (hh * fac_acc).simd_min(s(h_max));
        // rejected lanes
        let fac_rej = (s(0.9) * pw).simd_max(s(0.1));
        let hh_rej = hh * fac_rej;
        let reject = !accept & active;
        if (hh_rej.simd_lt(s(1e-12)) & reject).any() {
            let ta = t.to_array();
            let lane = (0..L).find(|&l| reject.test(l) && hh_rej[l] < 1e-12).unwrap();
            panic!("step size underflow at t={} (lane {lane})", ta[lane]);
        }
        *steps += active.select(s(1.0), s(0.0));
        t = accept.select(t_acc, t);
        for i in 0..N { y[i] = accept.select(ynew[i], y[i]); k1[i] = accept.select(k7[i], k1[i]); }
        h = accept.select(h_acc, h);
        hh = reject.select(hh_rej, hh);
        need_new_hh = accept;
        // output points reached (the scalar loop's `while t < t_out` exit)
        let reached = t.simd_ge(t_out) & accept;
        if reached.any() {
            let ta = t.to_array();
            let ya: [[f64; L]; N] = std::array::from_fn(|i| y[i].to_array());
            for l in 0..L {
                if !reached.test(l) { continue; }
                // exactly one grid point per reach: hh <= t_out - t, so a step never crosses two
                let yl: [f64; N] = std::array::from_fn(|i| ya[i][l]);
                out(l, ta[l], &yl);
                idx[l] += 1;
                if idx[l] >= times.len() { active.set(l, false); }
            }
            t_out = t_out_of(&idx);
        }
    }
    (y, nfev)
}

/// Lane version of `solve_two_stage`: constant influx to 5000 days, per-lane median of the sampled
/// states for t >= t_min_eval as the cyclic initial condition, then the cyclic model.
pub fn solve_two_stage_lanes(p: &LaneParams, t_end_cyclic: f64, dt: f64, t_min_eval: f64, rtol: f64, atol: f64, h_init: f64, h_max: f64, mut cyclic_out: impl FnMut(usize, f64, &[f64; N])) -> ([V; N], u64, [u64; L]) {
    let mut steps = V::splat(0.0);
    let n_const = (5000.0 / dt) as usize;
    let times: Vec<f64> = (1..=n_const).map(|i| i as f64 * dt).collect();
    let mut samples: Vec<Vec<[f64; N]>> = (0..L).map(|_| Vec::with_capacity(n_const)).collect();
    let y0: [V; N] = std::array::from_fn(|i| V::splat(crate::model::Y0[i]));
    let (_, nf1) = integrate_lanes(p, true, 0.0, y0, &times, rtol, atol, h_init, h_max, &mut steps, |l, t, y| { if t >= t_min_eval { samples[l].push(*y); } });
    let mut y0c = [V::splat(0.0); N];
    for i in 0..N {
        let mut v = [0.0; L];
        for l in 0..L {
            let mut col: Vec<f64> = samples[l].iter().map(|y| y[i]).collect();
            col.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let m = col.len();
            v[l] = if m % 2 == 1 { col[m / 2] } else { 0.5 * (col[m / 2 - 1] + col[m / 2]) };
        }
        y0c[i] = V::from_array(v);
    }
    let n_cyc = (t_end_cyclic / dt) as usize;
    let times: Vec<f64> = (1..=n_cyc).map(|i| i as f64 * dt).collect();
    let (y, nf2) = integrate_lanes(p, false, 0.0, y0c, &times, rtol, atol, h_init, h_max, &mut steps, |l, t, y| cyclic_out(l, t, y));
    (y, nf1 + nf2, steps.to_array().map(|x| x as u64))
}

/// `integrate_lanes` with the ode23s Rosenbrock step (stiff.rs); identical lane bookkeeping.
pub fn integrate_lanes_stiff(p: &LaneParams, const_mu_e: bool, t0: f64, y0: [V; N], times: &[f64], rtol: f64, atol: f64, h_init: f64, h_max: f64, steps: &mut V, mut out: impl FnMut(usize, f64, &[f64; N])) -> ([V; N], u64) {
    let s = V::splat;
    let inv = Inv::of(&p.base);
    let mu_c = s(p.base.mu_e);
    let mu = move |t: V| if const_mu_e { mu_c } else { vmath::mu_e_cyclic(t) };
    let dmu = move |t: V| if const_mu_e { s(0.0) } else { vmath::mu_e_cyclic_dt(t) };
    let mut t = s(t0);
    let mut y = y0;
    let mut h = s(h_init);
    let mut hh = s(0.0);
    let mut need_new_hh = M::splat(true);
    let mut idx = [0usize; L];
    let mut active = M::splat(true);
    let mut nfev: u64 = 0;
    let mut f0 = [s(0.0); N];
    rhs(t, &y, p, &inv, const_mu_e, &mut f0); nfev += 1;
    let t_out_of = |idx: &[usize; L]| V::from_array(std::array::from_fn(|l| if idx[l] < times.len() { times[idx[l]] } else { f64::INFINITY }));
    let mut t_out = t_out_of(&idx);
    while active.any() {
        let fresh = h.simd_min(t_out - t).simd_min(s(h_max));
        hh = need_new_hh.select(fresh, hh);
        hh = active.select(hh, s(0.0));
        let (ynew, fnew, err) = stiff::step(p, &inv, t, &y, hh, &f0, rtol, atol, &mu, &dmu);
        nfev += 2;
        let accept = err.simd_le(s(1.0)) & active;
        let pw = vmath::powf(accept.select(err.simd_max(s(1e-10)), err), accept.select(s(stiff::EXP_ACC), s(stiff::EXP_REJ)));
        let t_acc = t + hh;
        let fac_acc = (s(0.9) * pw).simd_min(s(5.0)).simd_max(s(0.2));
        let h_acc = (hh * fac_acc).simd_min(s(h_max));
        let fac_rej = (s(0.9) * pw).simd_max(s(0.1));
        let hh_rej = hh * fac_rej;
        let reject = !accept & active;
        if (hh_rej.simd_lt(s(1e-12)) & reject).any() {
            let ta = t.to_array();
            let lane = (0..L).find(|&l| reject.test(l) && hh_rej[l] < 1e-12).unwrap();
            panic!("step size underflow at t={} (lane {lane})", ta[lane]);
        }
        *steps += active.select(s(1.0), s(0.0));
        t = accept.select(t_acc, t);
        for i in 0..N { y[i] = accept.select(ynew[i], y[i]); f0[i] = accept.select(fnew[i], f0[i]); }
        h = accept.select(h_acc, h);
        hh = reject.select(hh_rej, hh);
        need_new_hh = accept;
        let reached = t.simd_ge(t_out) & accept;
        if reached.any() {
            let ta = t.to_array();
            let ya: [[f64; L]; N] = std::array::from_fn(|i| y[i].to_array());
            for l in 0..L {
                if !reached.test(l) { continue; }
                let yl: [f64; N] = std::array::from_fn(|i| ya[i][l]);
                out(l, ta[l], &yl);
                idx[l] += 1;
                if idx[l] >= times.len() { active.set(l, false); }
            }
            t_out = t_out_of(&idx);
        }
    }
    (y, nfev)
}

/// `solve_two_stage_lanes` with the stiff integrator.
pub fn solve_two_stage_lanes_stiff(p: &LaneParams, t_end_cyclic: f64, dt: f64, t_min_eval: f64, rtol: f64, atol: f64, h_init: f64, h_max: f64, mut cyclic_out: impl FnMut(usize, f64, &[f64; N])) -> ([V; N], u64, [u64; L]) {
    let mut steps = V::splat(0.0);
    let n_const = (5000.0 / dt) as usize;
    let times: Vec<f64> = (1..=n_const).map(|i| i as f64 * dt).collect();
    let mut samples: Vec<Vec<[f64; N]>> = (0..L).map(|_| Vec::with_capacity(n_const)).collect();
    let y0: [V; N] = std::array::from_fn(|i| V::splat(crate::model::Y0[i]));
    let (_, nf1) = integrate_lanes_stiff(p, true, 0.0, y0, &times, rtol, atol, h_init, h_max, &mut steps, |l, t, y| { if t >= t_min_eval { samples[l].push(*y); } });
    let mut y0c = [V::splat(0.0); N];
    for i in 0..N {
        let mut v = [0.0; L];
        for l in 0..L {
            let mut col: Vec<f64> = samples[l].iter().map(|y| y[i]).collect();
            col.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let m = col.len();
            v[l] = if m % 2 == 1 { col[m / 2] } else { 0.5 * (col[m / 2 - 1] + col[m / 2]) };
        }
        y0c[i] = V::from_array(v);
    }
    let n_cyc = (t_end_cyclic / dt) as usize;
    let times: Vec<f64> = (1..=n_cyc).map(|i| i as f64 * dt).collect();
    let (y, nf2) = integrate_lanes_stiff(p, false, 0.0, y0c, &times, rtol, atol, h_init, h_max, &mut steps, |l, t, y| cyclic_out(l, t, y));
    (y, nf1 + nf2, steps.to_array().map(|x| x as u64))
}
