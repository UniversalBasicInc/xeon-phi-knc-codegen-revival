# The tower of Babel question

> *Could a universal compute translator let SYCL — or any portable language — run on an
> ancient ISA?*

Not just in theory. **It existed, twice.** How each was built is the useful part.

## Two precedents

**Intel shipped OpenCL for KNC.** OpenCL Runtime 14 is the last version supporting the
coprocessor; support was deprecated from Intel SDK for OpenCL Applications 2019 and CPU
Runtime 18.1 onward. "Write portable kernels, run them on a Phi" was a shipping product.

**ISPC supported KNC — and the method is the lesson.** Jean-Luc Duprat added it in summer
2012. Because there was **no LLVM backend for KNC**, he routed through LLVM's C++ backend
to emit C++ intrinsics, then let Intel's compiler finish the job.

That is exactly the tower of Babel: IR → a language both ends already speak → native code.
A transpiler bridge, not a native code generator. It worked.

## Only one of four layers is actually missing

| Layer | For KNC | State |
|---|---|---|
| Language → IR (SYCL/OpenCL/CUDA → LLVM IR / SPIR-V) | generic | **solved, free, portable** |
| IR → machine code for the ISA | MVEX codegen | **the missing brick** |
| Runtime: buffers over PCIe, kernel launch, sync | SCIF / COI, in MPSS | exists — it *is* the driver revival |
| Device-side userland | k1om glibc + busybox | exists, in the card OS image |

Three of four are done or recoverable. And the fourth is smaller than it looks:

## MVEX vs EVEX — the hopeful detail

**KNC's MVEX prefix and AVX-512's EVEX prefix differ by a single bit for otherwise-identical
instructions.** EVEX is literally MVEX, tweaked, on the way from Larrabee to AVX-512.

So retargeting is not inventing a 512-bit vector backend from nothing — it is forking the
existing AVX-512F codegen path and re-encoding.

**The catch:** the *encodings* are close, the *instruction sets* are not. KNC lacks much of
AVX-512F, and carries things AVX-512 never had — the swizzle/broadcast/convert modifiers
carried in the MVEX payload, and the fast transcendental approximations. Close alphabet,
different vocabulary.

## Three routes, most to least tractable

**1. Transpile — the ISPC way.** LLVM IR → C++ intrinsics → k1om cross-compiler. No new
backend. Already proven on this exact card. *A weekend.*

**2. Scalar SYCL — the "it just runs" route.** AdaptiveCpp or triSYCL's library-only
backends lower SYCL to plain C++ + OpenMP; cross-compile that and real SYCL runs on the
card's ~240 threads. The vector unit sits idle, but it *runs*.
**The blocker is the C++ standard, not the ISA:** SYCL 2020 needs C++17, GCC 5.1.1 gives
C++14. Getting a newer k1om cross-compiler to exist is the gate. *A weekend, plus a
toolchain fight.*

**3. A real MVEX backend in LLVM.** Nobody has one. This is the version where SPIR-V and
SYCL land on the card *fast*, and it is genuine, publishable compiler work. *A research
project.*

## The caveat that governs all three

**Translation layers port correctness, not performance.**

Getting SYCL to *execute* on KNC is very achievable. Getting it to *beat the two E5-2683s
it is plugged into* is a different problem — that card needs ~240 threads, 512-bit vectors
and hand-done cache blocking to reach any fraction of peak, and no portable layer will
find that on its own.

This is not a hypothesis. It is the same lesson already learned one floor up: `sage-arc`,
`moe-arc` and `int2-arc` exist **precisely because** portable SYCL leaves Arc's XMX units
on the floor. Same lesson, harder version, on silicon with a third of the headroom.

## The strategic read

The tower already exists — it is LLVM plus SPIR-V. What is missing for any given abandoned
ISA is one last-mile brick. The reason nobody lays it is not difficulty; it is that the
payoff is zero for dead hardware.

Which is exactly the asymmetry worth exploiting. Lay the brick once, on the least
convenient target imaginable, and "any architecture" stops being a claim and becomes a
screenshot.
