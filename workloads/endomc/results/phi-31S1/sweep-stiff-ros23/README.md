# Xeon Phi 31S1: endomc `sweep-simd 101` with the Rosenbrock (ode23s) integrator — 2026-11-04/05

`ENDOMC_SOLVER=ros23 ENDOMC_TOL=1e-8`, grouped lanes, 228 threads, binary endomc-v5-k1om
(md5 9ddf9ffcafed7c4f3dc32f7981fc9037, commit 6481003).

| | result |
|---|---|
| wall time, full grid | **65.4 s** (467.9 sims/s) — DP5 best on this card: 829.4 s |
| wall time, n=31 | 8.9 s |
| output vs HQ (64-thread x86-64) ros23 run | byte-identical (`hq_sweep_101_ros23_tol1e-8.tsv`) |
| vs the DP5 HQ reference (`xeon-e5-2683v4/sweep_101x101x3.tsv`, tol 1e-6) | 30603/30603 classes agree; max rel diff EA_end 2.27e-3, M1_frac 9.3e-4 (one print digit), M2_frac 0 |
| max steps per cell | ~1.9e5 (DP5: 1.1e7) |
| HQ wall time, same run | 13.6 s on 64 threads |

Tolerance choice: at tol 1e-6 the second-order method is ~1.8 % off DP5 in EA_end on n=31; 1e-7 → 0.26 %;
1e-8 → 0.13 % (n=31) / 0.23 % (n=101). Class agreement is 100 % at every tolerance. The paper's own
solver was ode15s (stiff, variable order); DP5 and ros23 bracket it.

This is a change of numerical method, not of the model: results agree with the explicit solver to
the solver tolerance, not bit-for-bit. Both solvers remain in the binary (default DP5).
