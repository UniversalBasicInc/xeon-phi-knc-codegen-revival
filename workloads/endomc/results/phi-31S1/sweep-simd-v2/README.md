# Xeon Phi 31S1: endomc `sweep-simd 101` with the v2 vector math (2026-11-04)

Binary: endomc at commits 6474ce1 + 1b874ac, cross-compiled with cargo-knc (ShisoftResearch toolchain,
Rust nightly-2026-09-01, `-Cno-vectorize-loops -Cno-vectorize-slp`, explicit `std::simd` f64x8),
md5 88460787e2c6aca26594f3bbfd64e488. Card: 228 threads, MPSS 3.8.6 guest, the EPYC host CPU SLOT6.

| | result |
|---|---|
| grid | rho0 ∈ {0.01, 0.1, 0.2} × 101 log10 ω × 101 log10 β1 = 30,603 cells |
| wall time | 836.1 s (13 min 56 s) |
| rate | 36.6 sims/s |
| classes | low 1494 / transient 16461 / sustained 12648 |
| vs HQ reference (`xeon-e5-2683v4/sweep_101x101x3.tsv`, original scalar libm math) | 30603/30603 classes agree, max rel diff EA_end / M1_frac / M2_frac = 0 at printed precision |
| vs EPYC 7742 vector run (`epyc-7742/sweep_simd_101_epyc.tsv`) | identical, same |
| card power (thermal.log, 10 s samples) | mean 179 W, max 191 W; die max 74 °C |

Context: the v1 binary (no FMA, vector division via the toolchain's scalar runtime fallback, per-lane
libm sin and powf) ran 3 h 15 min on the same grid without finishing (killed, 152 W). The EPYC 7742
(128 threads, AVX2) does this grid in 184.7 s. So the card is now ~4.5× slower than the EPYC, from >50×.

Rate per rho0 block is lumpy: each block of 10,201 cells starts with its most expensive cells (low ω,
sustained disease), ~250 s, then the remaining ~8,000 cells clear in ~20 s. Lane groups of eight are
held by their slowest lane; grouping cells by expected cost is the next lever, with stack-spill reduction.
