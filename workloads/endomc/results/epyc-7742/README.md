# endomc full sweep on the EPYC host (EPYC 7742, 128 threads, AVX2, Rust nightly-2026-09-01, -C target-cpu=native)

Run 2026-11-03 evening, no coprocessor involved; the miner was running at nice 10 alongside.

| binary | time | rate | vs HQ scalar reference (`../xeon-e5-2683v4/sweep_101x101x3.tsv`) |
|---|---|---|---|
| `sweep-simd` (f64x8 lanes) | 184.7 s | 165.7 sims/s | 30,603/30,603 cells identical, all columns, 100 % class agreement |
| `sweep` (scalar) | 488.2 s | 62.7 sims/s | identical to the vector run and to HQ |

Class counts low/transient/sustained = 1494/16461/12648, identical to HQ.

Vector over scalar on this host (AVX2, f64x8 = two 256-bit ops): **2.64×** at equal results.
The HQ scalar reference (64 threads, 2× Xeon E5 v4) took 506.9 s; EPYC scalar 488.2 s.
