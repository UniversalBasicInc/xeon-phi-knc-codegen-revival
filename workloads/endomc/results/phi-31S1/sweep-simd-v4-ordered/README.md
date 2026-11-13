# Xeon Phi 31S1: endomc `sweep-simd 101` v4 (grouped lanes, cost-ordered) — 2026-11-04

Grouped solver (as v2) with cells scheduled most-expensive-first from the full-grid cost file.

| | result |
|---|---|
| wall time | 829.4 s (v2 grid order: 836.1 s; v3 refill: 1064.9 s) |
| output | byte-identical to the v2 card file; 30603/30603 vs HQ reference |

Scheduling is exhausted: the card delivers a fixed ~8.2e7 lane-steps/s on this loop, the grid needs
6.76e10 steps, floor ≈ 820 s, and v2 was already on it. Thread count (228/171/114 on n=31:
157.7/160.3/168.4 s) does not move it either. What is left without changing the numerics is the
per-step instruction count in the inner loop (spills, re-broadcast constants).
