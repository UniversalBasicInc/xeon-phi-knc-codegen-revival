# Xeon Phi 31S1: endomc `sweep-simd 101` v3 (per-lane refill, grid order) — 2026-11-04

Same grid and binary lineage as `../sweep-simd-v2` plus the lane scheduler (commit 1b490fb):
each lane takes the next cell the moment it finishes. Cells in grid order (no cost ordering).

| | result |
|---|---|
| wall time | 1064.9 s (v2: 836.1 s) |
| rate | 28.7 sims/s |
| output | byte-identical to the v2 card file; 30603/30603 vs HQ reference |
| per-cell cost (`sweep.tsv.cost`) | total 6.76e10 steps; max 1.11e7, median 12,469; top 25 % of cells = 93.6 % of steps; 6,969 cells over 5e6 steps |

Why refill lost time: the stiff cells (lowest ω) take ~1.1e7 steps each, so a block's wall time is
one such cell's serial step time. v2 ran them on ~150 of 228 threads with the other cores partly idle;
refill keeps all four threads of every core busy with stiff lanes, and on the in-order KNC core that
makes each thread step slower. Net: the critical path got longer. Scheduling cannot shorten a single
cell; only per-step speed (fewer threads per core, fewer spills) and longest-first ordering can.
