# dosemc — Monte Carlo photon dose in a slab phantom (water / lung)

Double precision, embarrassingly parallel (one history per task, one RNG stream per thread),
**zero dependencies** so the same crate cross-compiles for the Xeon Phi (k1om) with
ShisoftResearch's `cargo-knc`. Written 2026-11-02 as the first health-rooted Phi workload:
lung interfaces are where Monte Carlo dose calculation matters clinically.

```
cargo build --release
./target/release/dosemc --phantom water --energy 1.25 --histories 4000000 > pdd.tsv
./target/release/dosemc --phantom lung  --energy 1.25 --histories 4000000 > lung.tsv
```

Output: percent depth dose per 0.25 cm voxel with statistical uncertainty. Broad parallel
beam, 10×10 cm field by default (`--field`).

## Physics, v1 (honest scope)

- Photons: analogue transport; photoelectric (local electron), Compton (Klein–Nishina by
  Kahn's method, electron direction from kinematics), pair production (two 0.511 MeV
  annihilation photons). Cross sections: NIST XCOM water, log-log interpolated; lung =
  water composition at 0.26 g/cm³ (ICRU lung). **Tables transcribed by hand — re-check
  every value against NIST XCOM/ESTAR before any validation claim.**
- Electrons: energy deposited along a straight line of CSDA range (NIST ESTAR water,
  density scaled). No multiple scattering, no bremsstrahlung, no delta rays. This gives
  build-up and interface effects qualitatively; absolute depth of d_max is slightly deep
  and penumbra is absent. v2 = condensed-history electrons (Class II, Molière MS).

## Reference results (xeon-e5, dual E5-2683 v4, 64 threads, 4 M histories)

| Case | d_max | PDD 5 cm | PDD 10 cm | PDD 20 cm | rate |
|---|---|---|---|---|---|
| water, 1.25 MeV (Co-60-like) | 0.62 cm | 91.4 | 77.8 | 51.1 | 1.5e7 hist/s |
| water, 6 MeV monoenergetic | 2.88 cm | 96.6 | 88.3 | 72.2 | 2.3e7 hist/s |
| water/lung/water, 1.25 MeV | 0.62 cm | dose drops to 79.9 % entering lung at 3 cm, re-builds to ~91 % in lung, 98.3 % spike at the lung→water interface at 11 cm | | | 2.2e7 hist/s |

Expected for a parallel beam (no inverse square), these sit within ~5–10 % of published
TMR-like curves; the lung slab shows the textbook loss and re-establishment of electronic
equilibrium at both interfaces. Full curves in `results/xeon-e5-2683v4/`.

## Next

1. Re-verify the tables from NIST (script to fetch XCOM/ESTAR).
2. Condensed-history electrons (v2) and a measured-data comparison (e.g. the AAPM TG-105
   lung slab cases).
3. A tumour-in-lung phantom (small water sphere inside the lung slab).
4. Cross-compile for k1om; run 10⁸ histories on the Phi; compare curves bit-for-bit
   in statistical terms and histories/s per watt against the EPYC and the E5 pair.
5. The scene-renderer front end on the same transport kernel.
