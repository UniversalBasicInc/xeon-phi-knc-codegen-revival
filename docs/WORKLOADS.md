# What it could actually do

## The number that decides everything

KNC's fp64:fp32 ratio is **1:2**. Modern consumer and mining silicon deliberately cripples
fp64. So on double precision, a 2013 card beats every GPU in the fleet:

| | fp64 peak | Power | GFLOP/W (fp64) |
|---|---|---|---|
| Xeon Phi 7120P | ~1.2 TF | 300 W | ~4.0 |
| **2× E5-2683 v4 (the host itself)** | ~1.07 TF | ~240 W | ~4.5 |
| RTX 3060 | ~0.2 TF (1:32) | 170 W | ~1.2 |
| RTX A5000 | ~0.87 TF | 230 W | ~3.8 |
| CMP 170HX | fp64 heavily cut | 250 W | — |

Two readings, both true:

- **Good:** ~5–6× the fp64 of the 3060, and it beats an A5000 on fp64 per watt.
- **Brutal:** it is a **wash with the two CPUs already in the chassis**. You would be adding
  300 W to roughly double a box's fp64. On a solar budget that is not a win — and
  watts-per-answer is the entire differentiation of the surrounding work.

Treat this as settled. It is a demo and kernel-craft target, not a compute tier.

## Good fits

Embarrassingly parallel, dependency-light, dense fp64/fp32, high arithmetic intensity:

- **Monte Carlo** of any flavour — trivially parallel, near-zero dependencies, the ideal
  first real workload
- **N-body / astrophysics**
- **Stencil / PDE solvers**, lattice methods
- **Dense linear algebra** — and writing a DGEMM in IMCI intrinsics is precisely the
  kernel-craft exercise that makes this project worth doing

## Real code that already had KNC support

You asked about existing projects to build on — these genuinely shipped it:

- **GROMACS** — native KNC SIMD was in-tree until deprecated in 2021 and removed in 2022.
  Use the 2016–2018 line. This is the single strongest "real science, real port" candidate.
- **LAMMPS** — the USER-INTEL / INTEL package offloads neighbour-list and non-bonded force
  calculation to the Phi.
- **ISPC** — had a KNC target from 2012 (see `UNIVERSAL-TRANSLATOR.md`; the *how* matters).
- *Embree had a Xeon Phi backend in the 2.x line — unverified, check before relying on it.*

## Dead on arrival

- **Anything LLM or ML.** No int8/int4, no tensor units, 6–16 GB, zero framework support.
  Do not spend an hour on this.
- **Anything VM-based** (LHC@home's CernVM and friends) — impossible on k1om.
- **Anything with a modern dependency tree** — Boost, HDF5, netCDF, modern CMake. This is
  the filter that actually kills most candidate ports, not the ISA.
- **BOINC's client itself.** The card is an offload device behind a host-side application,
  not a BOINC host.
