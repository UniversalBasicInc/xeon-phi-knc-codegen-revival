# Build day — toolchain and first cross-compiled science on the 31S1

Staged 2026-11-03 (night before). Everything below runs on **the EPYC host as root**, under `/data`
(the root disk is nearly full). Steps marked **[you]** execute third-party build systems and are
run by hand; each is one command.

## Already in place

- SDK: `/data/knc/sdk/opt/mpss/3.8.6/sysroots/{k1om-mpss-linux,x86_64-mpsssdk-linux}`
  (unpacked from `mpss-sdk-k1om-3.8.6-1.x86_64.rpm`); `/opt/mpss -> /data/knc/sdk/opt/mpss`
  so the SDK's host binaries find their own `ld-linux` (their interpreter path is hard-wired).
- Sources at the toolchain's pinned revisions: `/data/knc/sources/{llvm-project,llama.cpp,rust}`
  and `gcc-13.4.0.tar.xz` (check `logs/fetch-sources.log` says `rust-ok`; the Rust clone is the slow one).
- Prerequisites: cmake, ninja, clang, patchelf (apt); Python 3.12 via uv
  (`/root/.local/share/uv/python/cpython-3.12-linux-x86_64-gnu/bin/python3.12`).
- Repos mirrored: `/data/knc/repos/{xeon-phi-toolchain,Intel-Phi-LLVM,Intel-Phi-3120A,Intel-Phi-AVX512,gcc-5.1.1-knc,xeon-phi-kernel-module}`.
- Guest `knc-guest` (Debian 10 + MPSS 3.8.6 + patched mic.ko), attach script `/data/knc/phi-attach.sh`.
- HQ reference results for both crates in `workloads/*/results/xeon-e5-2683v4/`; `workloads/compare_sweep.py`; `workloads/run_on_card.sh`.

## 0. Card in, known good (10 min)

```sh
/data/knc/phi-attach.sh            # finds the card, fixes hostdev, no-reset, vfio, fans full, starts guest
ssh knc-guest 'cat /sys/class/mic/mic0/state; micctrl -b mic0; micctrl -w mic0; micctrl -s; micsmc -t -f | head -12'
```
Expect: ready → online, die ~55 °C idle. If POST stays 30: the card took a hot reset → full power cycle.

## 1. Rust bootstrap **[you]** (5 min, network)

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
. "$HOME/.cargo/env"
rustup toolchain install nightly-2026-09-01 --profile minimal --component rust-src
```

## 2. ShisoftResearch toolchain **[you]** (LLVM ~1 h on 128 threads; Rust ~1 h; C++17 lib ~20 min)

```sh
export PATH="$HOME/.local/share/uv/python/cpython-3.12-linux-x86_64-gnu/bin:$HOME/.cargo/bin:$PATH"
export PHI_SDK_ROOT=/data/knc/sdk/opt/mpss/3.8.6
export PHI_JOBS=96
cd /data/knc/repos/xeon-phi-toolchain
python3.12 scripts/phi.py doctor
python3.12 scripts/phi.py test-host
python3.12 scripts/phi.py fetch llvm  --local /data/knc/sources/llvm-project
python3.12 scripts/phi.py fetch llama --local /data/knc/sources/llama.cpp
python3.12 scripts/phi.py fetch rust  --local /data/knc/sources/rust
python3.12 scripts/phi.py build-llvm      && python3.12 scripts/phi.py test-codegen
python3.12 scripts/phi.py build-runtime
bin/clang-knc -O2 examples/hello.c -o build/hello-knc        # first k1om binary
python3.12 scripts/phi.py build-cxx                           # C++17 for the card
python3.12 scripts/phi.py build-rust                          # cargo-knc
```
Smoke test on the card: `scp build/hello-knc` into the guest, then to `root@172.31.1.1`, run it.

## 3. The science crates on the card

```sh
cd /data/knc/repos/xeon-phi-toolchain
bin/cargo-knc build --manifest-path /data/knc/workloads/endomc/Cargo.toml --release
bin/cargo-knc build --manifest-path /data/knc/workloads/dosemc/Cargo.toml --release
```
(copy this repository's `workloads/` to `/data/knc/workloads` first.)
Then from the guest: `run_on_card.sh <endomc k1om binary> 101` → `compare_sweep.py` against
`endomc/results/xeon-e5-2683v4/sweep_101x101x3.tsv`. Expect 100 % class agreement and relative
differences ~1e-6 (solver tolerance), plus sims/s and watts from the thermal log. Same for dosemc
with `--histories 100000000`.

## 4. If time: Lasimeri's mainline-kernel stack **[you]**

`/data/knc/repos/Intel-Phi-3120A` — read `docs/` and `README.md` "Quick start" first; the
daemon matches `8086:225d` and ours is `225e`. Stop the guest (`virsh destroy knc-guest`), unbind
vfio, and follow their host-daemon path. A first boot of mainline Linux on a 31S1 is the result
to report upstream, success or failure.

## Watch

- Card thermals every run: `micsmc -t -f` in the guest (`run_on_card.sh` logs it every 10 s).
- the EPYC host root disk: keep every build under `/data`.

## Day 1 outcome (2026-11-03) and the morning sequence

Rungs 0–3 done: card online in SLOT6 only, toolchain built in 24 min (codegen 67/67), endomc and
dosemc cross-compiled (LTO off, panic=abort), endomc examples 6/8 bit-identical to HQ on the card.
Scalar 101×101×3 sweep on the card was killed at 3 h 50 min with no output (one card thread is 42×
slower than one HQ thread on scalar code). The lane-batched `sweep-simd` (f64x8, bit-identical to
scalar, 1.9× on AVX2) is built for the card and staged in the guest.

Morning:
```sh
/data/knc/phi-attach.sh                              # the EPYC host, card in SLOT6, POST 12 expected
ssh knc-guest /root/knc-guest/03-boot-card.sh                         # run by hand (MPSS tools)
ssh knc-guest 'scp -q /root/knc-guest/endomc-simd-k1om /root/knc-guest/dosemc-k1om root@172.31.1.1:/tmp/'   # card /tmp is RAM
ssh knc-guest 'setsid nohup bash /root/knc-guest/run_on_card.sh /root/knc-guest/endomc-simd-k1om 101 sweep-simd /root/knc-guest/card-results-simd > /root/knc-guest/run-simd.log 2>&1 < /dev/null &'
```
Then `compare_sweep.py workloads/endomc/results/xeon-e5-2683v4/sweep_101x101x3.tsv <card sweep.tsv>`, and
dosemc: `--phantom lung --energy 1.25 --histories 4000000 --threads 228` against `results/xeon-e5-2683v4/`.

## Day 2 outcome (2026-11-04): the vector sweep on the card

- First `sweep-simd 101` (v1 binary) ran 3 h 15 min unfinished and was killed. Disassembly: native
  512-bit vmulpd/vaddpd, but **zero** fused multiply-adds, and the toolchain lowers vector division
  to a scalar runtime fallback and sin/powf to per-lane libm calls (~400 scalar divides, 56 sin, 8 pow
  per step). The card drew 152 W, i.e. the vector unit was mostly waiting.
- v2 (`src/vmath.rs`): fused chains, reciprocal division (0 ulp), polynomial sinpi/log2/exp2/powf,
  rhs written once over a `Lane` trait for f64 and f64x8; sweeps write `<out>.partial` live and print
  progress every 1000 sims. KNC gotcha: `copysign` lowering asserts in the backend → sign-bit word ops.
- cargo-knc builds are scriptable; only the MPSS card boot is done by hand.
- Result: **836.1 s, 36.6 sims/s, all 30,603 cells identical to the HQ reference** (and the 8 examples
  now 8/8 bit-identical, was 6/8). 179 W mean, die 74 °C max. EPYC 7742: 184.7 s.
  Details: `workloads/endomc/results/phi-31S1/sweep-simd-v2/`.
- Next levers: group lane batches by expected cost (slowest lane holds the other seven); cut stack
  spills (3,249 vmovdqa32 vs 1,357 FMAs in the lane solver). Then dosemc lung phantom 4M histories.

### Night 2 (2026-11-04/05): the scheduling levers, measured

| run | time | notes |
|---|---|---|
| v2 grouped lanes, grid order | 836.1 s | 36.6 sims/s |
| v3 per-lane refill, grid order | 1064.9 s | mixed-stage lanes evaluate the cyclic influx polynomial every step |
| v4 grouped, cost-ordered (longest first) | 829.4 s | 36.8 sims/s |
| n=31 at 228 / 171 / 114 threads | 157.7 / 160.3 / 168.4 s | two threads per core already saturate the core |

All four byte-identical. Conclusion: the card is issue-bound at ~8.2e7 lane-steps/s; the grid needs
6.76e10 steps (top 25 % of cells = 94 %, stiff low-ω cells ~1.1e7 steps each). Scheduling cannot move
the floor (~820 s). The remaining lever that keeps the numerics is the instruction count per step,
and the disassembly says where it is: the KNC backend defines a single FMA form (`KNC_VFMADD132PD`,
X86InstrCompiler.td), so every accumulate-style FMA needs an extra register copy or stack reload of
the operand it destroys. In the lane solver: 737 FMAs, 3,149 vector moves. Adding the 213/231 forms
to the backend (IMCI has them; opcodes A8/B8 alongside 98) is the next real gain, and a finding worth
giving the toolchain authors with these numbers. Numerics changes (larger h_max + dense output,
stiff solver) were deliberately excluded for this test.

### Night 2, part 2: the stiff solver and the backend patch, measured

| run (card, 228 threads) | time | notes |
|---|---|---|
| DP5 vector, best scheduling (v4) | 829.4 s | bit-identical to HQ |
| **Rosenbrock ode23s, tol 1e-8 (v5)** | **65.4 s** | byte-identical to HQ's ros23 run; 30603/30603 classes agree with the DP5 reference, max rel diff EA_end 2.3e-3 |
| same, built through the patched backend | 63.5 s | bit-identical to the unpatched ros23 run |
| DP5 n=31, unpatched vs patched | 127.0 s vs 127.0 s | bit-identical; lane solver 11,276 → 11,007 instructions (−2.4 %), copies 762 → 395 |

Stiff solver (`src/stiff.rs`, `ENDOMC_SOLVER=ros23`, `ENDOMC_TOL`): analytic Jacobian, LU without
pivoting, Shampine–Reichelt pair, Lane-generic so scalar ≡ lanes. The stiffest cells drop from 1.1e7
to ~1.9e5 steps. Tolerance 1e-8 chosen so EA_end agrees with DP5 to ~0.2 % (1e-6 gives ~1.8 %).
This changes the numerical method; results agree to tolerance, not bit-for-bit. DP5 stays the default.

Backend patch (`docs/patches/knc-fma-213-231-forms.patch`, applied in the EPYC host `build/llvm`; backup in
`/data/knc/toolchain-backup`): adds KNC_VFMADD213/231 PS/PD, encoder opcodes A8/B8, latency
entries, and operand commutation with a form-switch table so the two-address pass ties the dying
operand. Verified on the card (C test, 512/512 lanes bit-exact; Rust object relinked via patched llc,
outputs bit-identical). Measured effect on this solver: none to ~3 %. Most remaining moves are genuine
spills (8 state vectors × 7 stages in 32 registers), not form copies. Worth offering upstream as a
correctness-neutral improvement, not as a speedup claim.

Card state at close: idle ~110 W, booted, the EPYC host fans full.
