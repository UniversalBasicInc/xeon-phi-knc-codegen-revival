mod phantom;
mod physics;
mod rng;
mod transport;

use phantom::Phantom;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut phantom_name = "water".to_string();
    let mut energy = 1.25_f64; // Co-60-like
    let mut histories: u64 = 1_000_000;
    let mut threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    let mut seed: u64 = 20261102;
    let mut field_half = 5.0_f64; // 10x10 cm field
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--phantom" => { phantom_name = args[i + 1].clone(); i += 1; }
            "--energy" => { energy = args[i + 1].parse().unwrap(); i += 1; }
            "--histories" => { histories = args[i + 1].parse().unwrap(); i += 1; }
            "--threads" => { threads = args[i + 1].parse().unwrap(); i += 1; }
            "--seed" => { seed = args[i + 1].parse().unwrap(); i += 1; }
            "--field" => { field_half = args[i + 1].parse::<f64>().unwrap() / 2.0; i += 1; }
            _ => { eprintln!("usage: dosemc [--phantom water|lung] [--energy MeV] [--histories N] [--threads T] [--seed S] [--field cm]"); std::process::exit(2); }
        }
        i += 1;
    }
    let ph = match phantom_name.as_str() { "lung" => Phantom::lung_slab(), _ => Phantom::water() };
    let per = histories / threads as u64;
    let t0 = Instant::now();
    let results: Vec<transport::Scorer> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..threads)
            .map(|t| { let ph = &ph; s.spawn(move || transport::run_histories(ph, energy, per, seed, t as u64, field_half)) })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut total = transport::Scorer::new(ph.n_voxels());
    for r in &results { total.merge(r); }
    let dt = t0.elapsed().as_secs_f64();

    // dose per voxel = energy / mass; normalise to the maximum → percent depth dose
    let n = total.histories as f64;
    let mut dose: Vec<f64> = Vec::with_capacity(total.dose.len());
    let mut err: Vec<f64> = Vec::with_capacity(total.dose.len());
    for (i, &e) in total.dose.iter().enumerate() {
        let z = (i as f64 + 0.5) * ph.dz_cm;
        let mass = ph.density_at(z) * ph.dz_cm; // per unit area
        let mean = e / n;
        let var = (total.dose2[i] / n - mean * mean).max(0.0) / n;
        dose.push(mean / mass);
        err.push(var.sqrt() / mass);
    }
    let dmax = dose.iter().cloned().fold(0.0, f64::max);
    let imax = dose.iter().position(|&d| d == dmax).unwrap_or(0);
    eprintln!(
        "dosemc: phantom={} E={} MeV histories={} threads={} time={:.2}s rate={:.3e} hist/s dmax at z={:.2} cm",
        phantom_name, energy, total.histories, threads, dt, total.histories as f64 / dt, (imax as f64 + 0.5) * ph.dz_cm
    );
    println!("# z_cm\tdensity\tPDD_percent\tuncert_percent");
    for i in 0..dose.len() {
        let z = (i as f64 + 0.5) * ph.dz_cm;
        println!("{:.3}\t{:.2}\t{:.3}\t{:.3}", z, ph.density_at(z), 100.0 * dose[i] / dmax, 100.0 * err[i] / dmax);
    }
}
