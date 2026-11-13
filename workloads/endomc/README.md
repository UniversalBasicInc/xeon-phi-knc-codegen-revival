# endomc — the Miller et al. 2025 immune–endometrial model, reimplemented

A zero-dependency Rust port of the first mechanistic mathematical model of endometriosis
lesion onset:

> Miller CM, Germano DPJ, Chenoweth AM, Holdsworth-Carson S. *Mathematical modelling of
> macrophage and natural killer cell immune response during early stages of peritoneal
> endometriosis lesion onset.* J R Soc Interface 2025; 22(229): 20250076.
> https://doi.org/10.1098/rsif.2025.0076 — code https://github.com/clairemiller/math_modelling_immune_endo (CC-BY-4.0)

Eight ODEs (resting/M1/M2 macrophages, resting/activated NK cells, eutopic, free and attached
endometrial cells), the cyclic menstrual influx, the authors' two-stage solve (constant influx
to steady state → median as initial condition → cyclic run), their disease classification
(low / transient / sustained attachment by EA at cycle ends), and their disease-free stability
condition. Parameters transcribed from `default_parameters.m`; right-hand sides from
`fn_immune_ode_system.m`. Solver: Dormand–Prince RK5(4), rtol = atol = 1e-6 (their ode15s
tolerances).

Why here: each simulation is independent, double precision, and tiny — a parameter sweep or a
virtual cohort is embarrassingly parallel. One copy per core on the Xeon Phi; 228 threads.

```
cargo build --release
./target/release/endomc examples results/examples          # the paper's named cases
./target/release/endomc sweep 101 results/sweep.tsv         # the 101×101×3 heatmap grid (30,603 runs)
```

## Reproduction (xeon-e5, 64 threads)

| Paper case | rho0 | omega | beta1 | Paper | This port |
|---|---|---|---|---|---|
| Default (Fig. 1b, 2) | 0.1 | 1e-5 | 1e-6 | transient attachment | **Transient**, peak EA 42.7, EA(cycle end) 0.015 |
| No attachment (Fig. 2) | 0.1 | 1e-4 | 1e-5 | low | **Low**, peak EA 0.34 |
| Sustained (Fig. 2) | 0.1 | 10^-5.7 | 1e-6 | sustained | **Sustained**, EA 4.59e8 (carrying capacity 1e9); stability condition false |

The stability condition (eq. 3.1, growth < clearance) is true for the first two and false for
the third, as in the paper. Retrograde-influx series (rho0 = 0 … 0.2, Fig. 3): peak EA rises
with rho0 (0 → 15 → 19 → 31 → 43 → 66) while EA at cycle ends stays ≲ 2 — more inflammation,
not more disease, which is the paper's Hypothesis A result. The subtle part reproduces too:
rho0 = 0.005 classifies as *sustained* at a low level (EA at cycle ends 1.99) while higher
influx is transient — the paper's own words are "low influx systems maintain a low level of
sustained attachment" and "high influx systems exhibit only transient attachment".

Time series for every case are in `results/xeon-e5-2683v4/examples/`.

## The sweep (Figs 4–5 grid: 101 × 101 × 3 = 30,603 runs, 507 s on 64 threads, 60 sims/s)

`results/xeon-e5-2683v4/sweep_101x101x3.tsv`. What the paper reports qualitatively, in numbers:

| rho0 | low | transient | sustained | sustained fraction | log10(omega) where disease begins, at log10(beta1) = −7 / −6 / −5 |
|---|---|---|---|---|---|
| 0.01 | 983 | 4344 | 4874 | 0.48 | −4.60 / −5.08 / −5.44 |
| 0.1 | 339 | 5952 | 3910 | 0.38 | −4.92 / −5.24 / −5.46 |
| 0.2 | 172 | 6165 | 3864 | 0.38 | −5.00 / −5.26 / −5.42 |

- Disease is driven by **clearance** (omega): at every beta1 the system goes healthy → diseased
  as omega falls, crossing near 10^-5.2 at the default beta1. Reduced **detection** (beta1)
  shifts that boundary over the whole two-decade beta1 range but by under one decade of omega
  — the paper's "wide range of beta1, narrow range of omega".
- More **retrograde influx** does not cause disease: the sustained fraction goes 0.48 → 0.38 →
  0.38 as rho0 rises (slightly protective, through immune activation), while low attachment
  almost vanishes (983 → 172) — more inflammation, not more lesions (Hypothesis A rejected).
- Across the boundary at beta1 = 1e-6, the **M1 fraction** of macrophages is 0.997 in diseased
  systems vs 0.001 in healthy ones and the M2 fraction falls 0.047 → 0.003: reduced clearance
  raises M1 and lowers M2, the paper's Fig. 5c trend (their argument for Hypothesis B2).

## Global sensitivity (new; the paper varied 3 parameters, this varies all 27)

`endomc sobol 512 3.0` — Saltelli design, N = 512, every parameter log-uniform within ×/÷ 3 of
its default (fractions capped at 1): 14,848 simulations, 221 s on 64 threads. Outputs:
log10 EA at the last cycle end, and the disease indicator; Sobol first-order (S1) and total
(ST) indices (Saltelli 2010 / Jansen). P(disease) over the cube = 0.48.
`results/xeon-e5-2683v4/sobol_N512_span3.tsv`.

| rank | parameter | meaning | ST (log EA) | ST (disease) |
|---|---|---|---|---|
| 1 | eta_e | attached-cell proliferation rate | 0.56 | 0.52 |
| 2 | omega | immune clearance rate | 0.44 | 0.47 |
| 3 | gamma | NK share of clearance | 0.25 | 0.22 |
| 4 | delta_k | NK-cell turnover | 0.22 | 0.39 |
| 5 | mc | macrophage carrying capacity | 0.19 | 0.20 |
| 6 | mu_k | NK supply from circulation | 0.13 | 0.17 |
| 7 | theta_k | NK activation by M1 | 0.11 | 0.14 |
| 8 | beta1 | macrophage detection of ectopic cells | 0.11 | 0.14 |
| … | rho0, mu_e (influx) | | < 0.01 | ≤ 0.04 |
| … | beta2, beta12, beta21, cm2, cka, theta_m, mu_m, ec | macrophage polarisation / M2 upregulation | ≤ 0.003 | ≤ 0.01 |

Reading, with the caveats below:

- **Lesion-cell proliferation and immune clearance share control** of onset (ST ≈ 0.5 each),
  and the paper's headline that clearance beats detection holds: beta1 ranks eighth.
- **The NK-cell axis is the next tier** — gamma, delta_k, mu_k, theta_k together outrank
  every macrophage-specific parameter. Within this model, how many activated NK cells there
  are and how long they live matters more than how macrophages polarise.
- **Macrophage M1/M2 polarisation rates and the M2 → proliferation constant are inert**
  (ST < 0.01): the model's M1/M2 narrative is carried entirely through omega's
  (1−gamma)·M1 term and mc, not through the polarisation kinetics.
- **Retrograde influx is irrelevant to onset** (rho0, mu_e ≈ 0), agreeing with the paper.
- Large ST − S1 gaps everywhere: the system is strongly interactive (bistability), so
  one-at-a-time sweeps understate what matters.

Caveats: ×/÷ 3 is a first-pass range, not literature-derived; N = 512 gives S1 noise of a few
hundredths (small negatives are noise); results are about *this* model's structure, not the
disease. Both are exactly the kind of thing to put in front of the authors.

## Next

1. Compare the sweep heat map with Fig. 4b/5 cell by cell (their CSVs are regenerable from
   their MATLAB; ours is `results/xeon-e5-2683v4/sweep_101x101x3.tsv`).
2. Sensitivity with literature-derived ranges per parameter, N = 4096, and a second output
   (hysteresis width); Sobol' quasi-random sequences instead of pseudo-random.
3. **Virtual cohort**: sample patient-level parameters from literature ranges, report the
   distribution of outcomes and the hysteresis width.
4. Cross-compile for k1om; run the cohort on the Phi.
5. Stage 2 (separate crate): a 2D lattice lesion-morphology model — nobody has one.
