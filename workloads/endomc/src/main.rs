#![feature(portable_simd)]
mod model;
mod ode;
mod sobol;
mod simd;
mod vmath;
mod stiff;

use model::{Params, N, Y0};
use ode::Solver;
use std::io::Write;
use std::time::Instant;

/// The authors' two-stage protocol: constant influx to 5000 days, take the median of the
/// sampled states for t >= t_min_eval as the cyclic initial condition, then run the cyclic model.
fn solve_two_stage(p: &Params, t_end_cyclic: f64, dt: f64, t_min_eval: f64, mut cyclic_out: impl FnMut(f64, &[f64; N])) -> ([f64; N], u64) {
    let s = Solver::default();
    let stiff = stiff_mode();
    let integ = |p: &Params, c: bool, t0: f64, y0: [f64; N], times: &[f64], out: &mut dyn FnMut(f64, &[f64; N])| if stiff { s.integrate_stiff(p, c, t0, y0, times, out) } else { s.integrate(p, c, t0, y0, times, out) };
    let n_const = (5000.0 / dt) as usize;
    let times: Vec<f64> = (1..=n_const).map(|i| i as f64 * dt).collect();
    let mut samples: Vec<[f64; N]> = Vec::with_capacity(n_const);
    let (_, nf1) = integ(p, true, 0.0, Y0, &times, &mut |t, y| { if t >= t_min_eval { samples.push(*y); } });
    let mut y0c = [0.0; N];
    for i in 0..N {
        let mut col: Vec<f64> = samples.iter().map(|y| y[i]).collect();
        col.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let m = col.len();
        y0c[i] = if m % 2 == 1 { col[m / 2] } else { 0.5 * (col[m / 2 - 1] + col[m / 2]) };
    }
    let n_cyc = (t_end_cyclic / dt) as usize;
    let times: Vec<f64> = (1..=n_cyc).map(|i| i as f64 * dt).collect();
    let (_, nf2) = integ(p, false, 0.0, y0c, &times, &mut |t, y| cyclic_out(t, y));
    (y0c, nf1 + nf2)
}

/// ENDOMC_SOLVER=ros23 selects the Rosenbrock (stiff) integrator in every mode; default Dormand–Prince.
fn stiff_mode() -> bool { std::env::var("ENDOMC_SOLVER").map(|v| v == "ros23").unwrap_or(false) }

#[derive(Clone, Copy, PartialEq, Debug)]
enum Class { Low, Transient, Sustained }

/// Paper's definitions with threshold eps and transient tau: sustained if EA at cycle ends > eps.
fn classify(ea_series: &[(f64, f64)], eps: f64, tau: f64) -> Class {
    let mut peak_above = false;
    let mut cycle_end_above = false;
    for &(t, ea) in ea_series {
        if t <= tau { continue; }
        if ea > eps { peak_above = true; }
        let k = (t / 28.0).round();
        if (t - 28.0 * k).abs() < 1e-9 && k >= 1.0 && ea > eps { cycle_end_above = true; }
    }
    if cycle_end_above { Class::Sustained } else if peak_above { Class::Transient } else { Class::Low }
}

fn run_example(name: &str, p: Params, out_dir: &str) {
    let mut ts: Vec<(f64, [f64; N])> = Vec::new();
    let t0 = Instant::now();
    let (y0c, nfev) = solve_two_stage(&p, 3.0 * 13.0 * 28.0, 0.5, 0.0, |t, y| ts.push((t, *y)));
    let dt = t0.elapsed().as_secs_f64();
    let ea: Vec<(f64, f64)> = ts.iter().map(|(t, y)| (*t, y[7])).collect();
    let cls = classify(&ea, 1.0, 28.0 * 13.0);
    let last = ts.last().unwrap().1;
    let (ok, g, c) = model::condition_low_ea(&last, &p);
    let ea_cycle_ends: Vec<String> = ts.iter().filter(|(t, _)| ((t / 28.0).round() * 28.0 - t).abs() < 1e-9 && *t > 28.0 * 36.0).map(|(_, y)| format!("{:.3}", y[7])).collect();
    println!("{name:<22} rho0={:<6} omega={:<9.3e} beta1={:<9.3e} -> {:?}; EA at last cycle ends: [{}]; peak EA={:.2}; cond(growth<clear)={} ({:.3e} vs {:.3e}); y0c EA={:.2}; {:.0} RHS evals, {:.2}s",
        p.rho0, p.omega, p.beta1, cls, ea_cycle_ends.join(", "), ea.iter().map(|e| e.1).fold(0.0, f64::max), ok, g, c, y0c[7], nfev as f64, dt);
    std::fs::create_dir_all(out_dir).unwrap();
    let mut f = std::fs::File::create(format!("{out_dir}/{name}_timeseries.tsv")).unwrap();
    writeln!(f, "t\tM0\tM1\tM2\tK0\tKA\tE0\tEF\tEA").unwrap();
    for (t, y) in &ts { writeln!(f, "{t}\t{}", y.iter().map(|v| format!("{v:.6e}")).collect::<Vec<_>>().join("\t")).unwrap(); }
}

type Row = (f64, f64, f64, Class, f64, f64, f64);

/// Live progress for the sweeps: every finished row goes to `<out>.partial` immediately (the card
/// run gives no other sign of life), and a rate/ETA line goes to stderr every 1000 simulations.
struct Progress { done: std::sync::atomic::AtomicUsize, total: usize, t0: Instant, partial: std::sync::Mutex<std::io::BufWriter<std::fs::File>>, partial_path: String }

impl Progress {
    fn new(total: usize, out_path: &str) -> Progress {
        let partial_path = format!("{out_path}.partial");
        let mut f = std::io::BufWriter::new(std::fs::File::create(&partial_path).unwrap());
        writeln!(f, "rho0\tlog10_omega\tlog10_beta1\tclass\tEA_end\tM1_frac\tM2_frac").unwrap();
        f.flush().unwrap();
        Progress { done: std::sync::atomic::AtomicUsize::new(0), total, t0: Instant::now(), partial: std::sync::Mutex::new(f), partial_path }
    }
    fn record(&self, rows: &[Row]) {
        {
            let mut f = self.partial.lock().unwrap();
            for r in rows { writeln!(f, "{}\t{:.3}\t{:.3}\t{:?}\t{:.4e}\t{:.4}\t{:.4}", r.0, r.1, r.2, r.3, r.4, r.5, r.6).unwrap(); }
            f.flush().unwrap();
        }
        let prev = self.done.fetch_add(rows.len(), std::sync::atomic::Ordering::Relaxed);
        let d = prev + rows.len();
        if d / 1000 != prev / 1000 || d == self.total {
            let el = self.t0.elapsed().as_secs_f64();
            let rate = d as f64 / el;
            eprintln!("  {d}/{} in {el:.0}s ({rate:.1} sims/s, ~{:.0}s left)", self.total, (self.total - d) as f64 / rate);
        }
    }
    /// final sorted file; the partial file is removed
    fn finish(self, label: &str, threads: usize, results: &[Vec<Row>], out_path: &str) {
        let dt = self.t0.elapsed().as_secs_f64();
        let mut rows: Vec<&Row> = results.iter().flatten().collect();
        rows.sort_by(|a, b| (a.0, a.1, a.2).partial_cmp(&(b.0, b.1, b.2)).unwrap());
        let mut f = std::fs::File::create(out_path).unwrap();
        writeln!(f, "rho0\tlog10_omega\tlog10_beta1\tclass\tEA_end\tM1_frac\tM2_frac").unwrap();
        let mut counts = [0usize; 3];
        for r in rows {
            counts[match r.3 { Class::Low => 0, Class::Transient => 1, Class::Sustained => 2 }] += 1;
            writeln!(f, "{}\t{:.3}\t{:.3}\t{:?}\t{:.4e}\t{:.4}\t{:.4}", r.0, r.1, r.2, r.3, r.4, r.5, r.6).unwrap();
        }
        let _ = std::fs::remove_file(&self.partial_path);
        eprintln!("{label}: {} simulations on {threads} threads in {dt:.1}s ({:.1} sims/s); low={} transient={} sustained={}", self.total, self.total as f64 / dt, counts[0], counts[1], counts[2]);
    }
}

fn sweep_grid(n_samples: usize) -> Vec<(f64, f64, f64)> {
    // paper: omega in 10^[-6,-4], beta1 in 10^[-7,-5], rho0 in {0.01, 0.1, 0.2}; cyclic 14 cycles
    let rho0s = [0.01, 0.1, 0.2];
    let mut jobs: Vec<(f64, f64, f64)> = Vec::new();
    for &r in &rho0s { for i in 0..n_samples { for j in 0..n_samples {
        let lo = -6.0 + 2.0 * i as f64 / (n_samples - 1) as f64;
        let lb = -7.0 + 2.0 * j as f64 / (n_samples - 1) as f64;
        jobs.push((r, lo, lb));
    } } }
    jobs
}

fn run_sweep(threads: usize, n_samples: usize, out_path: &str) {
    let jobs = sweep_grid(n_samples);
    let prog = Progress::new(jobs.len(), out_path);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results: Vec<Vec<Row>> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..threads).map(|_| { let jobs = &jobs; let next = &next; let prog = &prog; s.spawn(move || {
            let mut mine = Vec::new();
            loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if i >= jobs.len() { break; }
                let (r, lo, lb) = jobs[i];
                let p = Params { rho0: r, omega: 10f64.powf(lo), beta1: 10f64.powf(lb), ..Default::default() };
                let mut ea: Vec<(f64, f64)> = Vec::new();
                let mut last = [0.0; N];
                solve_two_stage(&p, 14.0 * 28.0, 0.5, 2000.0, |t, y| { ea.push((t, y[7])); last = *y; });
                let cls = classify(&ea, 1.0, 28.0 * 2.0);
                let ea_end = ea.last().unwrap().1;
                let mtot = last[0] + last[1] + last[2];
                let row = (r, lo, lb, cls, ea_end, last[1] / mtot, last[2] / mtot);
                prog.record(std::slice::from_ref(&row));
                mine.push(row);
            }
            mine
        }) }).collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    prog.finish("sweep", threads, &results, out_path);
}

/// Same grid and outputs as `run_sweep`, eight cells per vector register with per-lane refill
/// (see simd.rs). `order`: optional `<out>.cost` file from a previous run; cells are then scheduled
/// most expensive first (longest-processing-time order), which tightens the tail.
fn run_sweep_simd_refill(threads: usize, n_samples: usize, out_path: &str, order: Option<&str>) {
    let grid = sweep_grid(n_samples);
    let mut jobs: Vec<simd::Job> = grid.iter().enumerate().map(|(id, &(r, lo, lb))| simd::Job { id, rho0: r, omega: 10f64.powf(lo), beta1: 10f64.powf(lb) }).collect();
    if let Some(path) = order {
        let mut cost: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
        for line in std::fs::read_to_string(path).unwrap().lines().skip(1) {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() == 4 { cost.insert(format!("{}\t{}\t{}", f[0], f[1], f[2]), f[3].parse().unwrap_or(0)); }
        }
        let key = |j: &simd::Job| { let (r, lo, lb) = grid[j.id]; format!("{}\t{:.3}\t{:.3}", r, lo, lb) };
        let known = jobs.iter().filter(|j| cost.contains_key(&key(j))).count();
        jobs.sort_by_key(|j| std::cmp::Reverse(cost.get(&key(j)).copied().unwrap_or(0)));
        eprintln!("sweep-simd: ordering {known}/{} cells by cost from {path}", jobs.len());
    }
    let prog = Progress::new(jobs.len(), out_path);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let s = Solver::default();
    let base = Params::default();
    let cost_rows: std::sync::Mutex<Vec<(usize, u64)>> = std::sync::Mutex::new(Vec::with_capacity(jobs.len()));
    let results: Vec<Vec<Row>> = std::thread::scope(|sc| {
        let hs: Vec<_> = (0..threads).map(|_| { let jobs = &jobs; let grid = &grid; let next = &next; let s = &s; let prog = &prog; let base = &base; let cost_rows = &cost_rows; sc.spawn(move || {
            let mut mine = Vec::new();
            let next_job = || { let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed); jobs.get(i).copied() };
            simd::run_jobs(base, 14.0 * 28.0, 0.5, 2000.0, s.rtol, s.atol, s.h_init, s.h_max, next_job, |d| {
                let (r, lo, lb) = grid[d.id];
                let cls = classify(&d.ea, 1.0, 28.0 * 2.0);
                let ea_end = d.ea.last().unwrap().1;
                let mtot = d.last[0] + d.last[1] + d.last[2];
                let row = (r, lo, lb, cls, ea_end, d.last[1] / mtot, d.last[2] / mtot);
                prog.record(std::slice::from_ref(&row));
                cost_rows.lock().unwrap().push((d.id, d.steps));
                mine.push(row);
            });
            mine
        }) }).collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    // per-cell step counts, for ordering the next run
    let mut cr = cost_rows.into_inner().unwrap();
    cr.sort();
    let mut f = std::fs::File::create(format!("{out_path}.cost")).unwrap();
    writeln!(f, "rho0\tlog10_omega\tlog10_beta1\tsteps").unwrap();
    for (id, st) in cr { let (r, lo, lb) = grid[id]; writeln!(f, "{}\t{:.3}\t{:.3}\t{st}", r, lo, lb).unwrap(); }
    prog.finish("sweep-simd-refill", threads, &results, out_path);
}


/// Default vector sweep: lane groups of eight through both stages (simd::solve_two_stage_lanes),
/// optional most-expensive-first ordering from a previous run's `<out>.cost`.
fn run_sweep_simd(threads: usize, n_samples: usize, out_path: &str, order: Option<&str>) {
    use simd::{LaneParams, L};
    let grid = sweep_grid(n_samples);
    let mut ids: Vec<usize> = (0..grid.len()).collect();
    if let Some(path) = order {
        let mut cost: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
        for line in std::fs::read_to_string(path).unwrap().lines().skip(1) {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() == 4 { cost.insert(format!("{}\t{}\t{}", f[0], f[1], f[2]), f[3].parse().unwrap_or(0)); }
        }
        let key = |id: usize| { let (r, lo, lb) = grid[id]; format!("{}\t{:.3}\t{:.3}", r, lo, lb) };
        let known = ids.iter().filter(|&&id| cost.contains_key(&key(id))).count();
        ids.sort_by_key(|&id| std::cmp::Reverse(cost.get(&key(id)).copied().unwrap_or(0)));
        eprintln!("sweep-simd: ordering {known}/{} cells by cost from {path}", ids.len());
    }
    let prog = Progress::new(grid.len(), out_path);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let s = Solver::default();
    let cost_rows: std::sync::Mutex<Vec<(usize, u64)>> = std::sync::Mutex::new(Vec::with_capacity(grid.len()));
    let results: Vec<Vec<Row>> = std::thread::scope(|sc| {
        let hs: Vec<_> = (0..threads).map(|_| { let ids = &ids; let grid = &grid; let next = &next; let s = &s; let prog = &prog; let cost_rows = &cost_rows; sc.spawn(move || {
            let mut mine = Vec::new();
            loop {
                let i0 = next.fetch_add(L, std::sync::atomic::Ordering::Relaxed);
                if i0 >= ids.len() { break; }
                let n_real = L.min(ids.len() - i0);
                let lane_job = |l: usize| grid[ids[i0 + l.min(n_real - 1)]];   // pad the tail with the last job
                let lp = LaneParams {
                    base: Params::default(),
                    rho0: std::simd::f64x8::from_array(std::array::from_fn(|l| lane_job(l).0)),
                    omega: std::simd::f64x8::from_array(std::array::from_fn(|l| 10f64.powf(lane_job(l).1))),
                    beta1: std::simd::f64x8::from_array(std::array::from_fn(|l| 10f64.powf(lane_job(l).2))),
                };
                let mut ea: Vec<Vec<(f64, f64)>> = (0..L).map(|_| Vec::new()).collect();
                let mut last = [[0.0; N]; L];
                let (_, _, steps) = if stiff_mode() { simd::solve_two_stage_lanes_stiff(&lp, 14.0 * 28.0, 0.5, 2000.0, s.rtol, s.atol, s.h_init, s.h_max, |l, t, y| { ea[l].push((t, y[7])); last[l] = *y; }) } else { simd::solve_two_stage_lanes(&lp, 14.0 * 28.0, 0.5, 2000.0, s.rtol, s.atol, s.h_init, s.h_max, |l, t, y| { ea[l].push((t, y[7])); last[l] = *y; }) };
                let mut rows = Vec::with_capacity(n_real);
                let mut cr = Vec::with_capacity(n_real);
                for l in 0..n_real {
                    let id = ids[i0 + l];
                    let (r, lo, lb) = grid[id];
                    let cls = classify(&ea[l], 1.0, 28.0 * 2.0);
                    let ea_end = ea[l].last().unwrap().1;
                    let mtot = last[l][0] + last[l][1] + last[l][2];
                    rows.push((r, lo, lb, cls, ea_end, last[l][1] / mtot, last[l][2] / mtot));
                    cr.push((id, steps[l]));
                }
                prog.record(&rows);
                cost_rows.lock().unwrap().extend(cr);
                mine.extend(rows);
            }
            mine
        }) }).collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut cr = cost_rows.into_inner().unwrap();
    cr.sort();
    let mut f = std::fs::File::create(format!("{out_path}.cost")).unwrap();
    writeln!(f, "rho0\tlog10_omega\tlog10_beta1\tsteps").unwrap();
    for (id, st) in cr { let (r, lo, lb) = grid[id]; writeln!(f, "{}\t{:.3}\t{:.3}\t{st}", r, lo, lb).unwrap(); }
    prog.finish("sweep-simd", threads, &results, out_path);
}

/// Saltelli design: A, B (n × 27 each) and 27 AB_i matrices → n(27+2) simulations.
fn run_sobol(threads: usize, n: usize, span: f64, out_path: &str) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let k = 27;
    // quasi-random-ish: plain xoshiro uniforms are fine for N≥512 with Jansen estimators
    let mut rng_state: u64 = 0x9E3779B97F4A7C15 ^ (n as u64);
    let mut uni = move || { rng_state ^= rng_state << 13; rng_state ^= rng_state >> 7; rng_state ^= rng_state << 17; (rng_state >> 11) as f64 / 9007199254740992.0 };
    let a: Vec<[f64; 27]> = (0..n).map(|_| { let mut u = [0.0; 27]; for x in u.iter_mut() { *x = uni(); } u }).collect();
    let b: Vec<[f64; 27]> = (0..n).map(|_| { let mut u = [0.0; 27]; for x in u.iter_mut() { *x = uni(); } u }).collect();
    // job list: (matrix id, row): id 0 = A, 1 = B, 2+i = AB_i
    let mut jobs: Vec<(usize, usize)> = Vec::with_capacity(n * (k + 2));
    for m in 0..(k + 2) { for r in 0..n { jobs.push((m, r)); } }
    let total = jobs.len();
    let t0 = Instant::now();
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    // outputs: (log10 EA_end clipped at 1e-3, disease 0/1)
    let results: Vec<Vec<(usize, usize, f64, f64)>> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..threads).map(|_| { let (jobs, a, b, next, done) = (&jobs, &a, &b, &next, &done); s.spawn(move || {
            let mut mine = Vec::new();
            loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= jobs.len() { break; }
                let (m, r) = jobs[i];
                let mut u = if m == 1 { b[r] } else { a[r] };
                if m >= 2 { u[m - 2] = b[r][m - 2]; }
                let p = sobol::set(&sobol::map(&u, span));
                let mut ea: Vec<(f64, f64)> = Vec::new();
                solve_two_stage(&p, 14.0 * 28.0, 0.5, 2000.0, |t, y| ea.push((t, y[7])));
                let cls = classify(&ea, 1.0, 28.0 * 2.0);
                let ea_end = ea.last().unwrap().1.max(1e-3);
                mine.push((m, r, ea_end.log10(), if cls == Class::Sustained { 1.0 } else { 0.0 }));
                let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                if d % 2000 == 0 { eprintln!("  {d}/{total} ({:.0}s)", t0.elapsed().as_secs_f64()); }
            }
            mine
        }) }).collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let dt = t0.elapsed().as_secs_f64();
    let mut y_log = vec![vec![0.0; n]; k + 2];
    let mut y_dis = vec![vec![0.0; n]; k + 2];
    for r in results.iter().flatten() { y_log[r.0][r.1] = r.2; y_dis[r.0][r.1] = r.3; }
    let ind_log = sobol::sobol(&y_log[0], &y_log[1], &y_log[2..]);
    let ind_dis = sobol::sobol(&y_dis[0], &y_dis[1], &y_dis[2..]);
    let p_dis = (y_dis[0].iter().sum::<f64>() + y_dis[1].iter().sum::<f64>()) / (2.0 * n as f64);
    eprintln!("sobol: {total} simulations on {threads} threads in {dt:.0}s; N={n} span=x/÷{span}; P(disease)={p_dis:.3}; var(log10 EA)={:.3}", ind_log.var);
    let mut f = std::fs::File::create(out_path).unwrap();
    writeln!(f, "parameter	S1_logEA	ST_logEA	S1_disease	ST_disease").unwrap();
    let mut order: Vec<usize> = (0..k).collect();
    order.sort_by(|&i, &j| ind_log.st[j].partial_cmp(&ind_log.st[i]).unwrap());
    for i in order {
        writeln!(f, "{}	{:.4}	{:.4}	{:.4}	{:.4}", sobol::NAMES[i], ind_log.s1[i], ind_log.st[i], ind_dis.s1[i], ind_dis.st[i]).unwrap();
        eprintln!("  {:<9} S1={:>7.3} ST={:>7.3} | disease S1={:>7.3} ST={:>7.3}", sobol::NAMES[i], ind_log.s1[i], ind_log.st[i], ind_dis.s1[i], ind_dis.st[i]);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(|s| s.as_str()).unwrap_or("examples");
    match mode {
        "examples" => {
            let out = args.get(2).map(|s| s.as_str()).unwrap_or("results/examples");
            let base = Params::default();
            run_example("default_transient", base, out);
            run_example("no_attachment", Params { omega: 1e-4, beta1: 1e-5, ..base }, out);
            run_example("sustained", Params { omega: 10f64.powf(-5.7), beta1: 1e-6, ..base }, out);
            for &r in &[0.0, 0.005, 0.01, 0.05, 0.2] { run_example(&format!("rho0_{r}"), Params { rho0: r, ..base }, out); }
        }
        "sobol" => {
            let n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(512);
            let span: f64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(3.0);
            let out = args.get(4).map(|s| s.as_str()).unwrap_or("results/sobol.tsv");
            let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
            std::fs::create_dir_all(std::path::Path::new(out).parent().unwrap()).unwrap();
            run_sobol(threads, n, span, out);
        }
        "sweep" => {
            let n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(101);
            let out = args.get(3).map(|s| s.as_str()).unwrap_or("results/sweep.tsv");
            let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
            std::fs::create_dir_all(std::path::Path::new(out).parent().unwrap()).unwrap();
            run_sweep(threads, n, out);
        }
        "sweep-simd" | "sweep-simd-refill" => {
            let n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(101);
            let out = args.get(3).map(|s| s.as_str()).unwrap_or("results/sweep.tsv");
            let order = args.get(4).map(|s| s.as_str());
            let threads = std::env::var("ENDOMC_THREADS").ok().and_then(|v| v.parse().ok()).unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1));
            std::fs::create_dir_all(std::path::Path::new(out).parent().unwrap()).unwrap();
            if mode == "sweep-simd" { run_sweep_simd(threads, n, out, order) } else { run_sweep_simd_refill(threads, n, out, order) }
        }
        _ => { eprintln!("usage: endomc examples [out_dir] | sweep [n_samples] [out.tsv] | sweep-simd|sweep-simd-refill [n_samples] [out.tsv] [order.cost] | sobol [N] [span] [out.tsv]"); std::process::exit(2); }
    }
}
