# endomc examples: Xeon Phi 31S1 (k1om, ShisoftResearch toolchain) vs HQ x86-64 reference

Run 2026-11-03, binary `endomc` cross-compiled with `cargo-knc` (nightly-2026-09-01 std, LTO off,
panic=abort, no auto-vectorisation flags from the toolchain), executed on the card over the MPSS pair
link. Reference: `../xeon-e5-2683v4/examples/` (HQ, same source, native x86-64).

| example | rows | result |
|---|---|---|
| default_transient | 2184 | bit-identical |
| rho0_0.005 / 0.01 / 0.05 / 0.2 / 0 | 2184 each | bit-identical |
| sustained | 2184 | max rel diff 2.0e-6 (solver rtol/atol = 1e-6) |
| no_attachment | 2184 | 753 rows differ; E0 max rel 7.7e-6; EA/EF differences are at |EA| ~1e-8, below atol (true value 0) |

Interpretation: identical control flow and arithmetic order on six cases; the two adaptive cases
where the solution grazes zero pick up last-bit differences from the card's fp64 codegen (fused
multiply-add available on KNC) which the step controller then amplifies to ~1e-6. No classification
is affected (ε = 1 on EA at cycle ends).
