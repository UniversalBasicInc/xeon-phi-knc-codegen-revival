# Software archaeology

Everything needed to run a KNC card still exists in 2026. Nothing here is vendor-supported;
all of it is community-preserved or archived. **Mirror it all to `/mnt/backup` now** — this
is exactly the kind of material that vanishes.

## The four pieces

| Piece | Source | State |
|---|---|---|
| Kernel driver (`mic.ko`, SCIF) | `github.com/subgeniuskitty/xeon-phi-kernel-module` | MPSS 3.8.6 driver ported; **tested to Linux 4.19.181** (Debian 10 buster) |
| MPSS build patches | `jjkeijser.github.io/mpss` | MPSS 3.8.4 / 3.8.5 / 3.8.6; CentOS 7.5–7.9 (3.10 kernels), CentOS 8.2–8.5 (4.18 kernels) |
| Full MPSS 3.8.6 release | `archive.org/details/intel-mpss-3.8.6` | **complete** — see file list below |
| k1om cross-compiler | `github.com/apc-llc/gcc-5.1.1-knc` | GCC 5.1.1; README claims 512-bit vector builtins (`__m512d`) available |

### MPSS 3.8.6 archive contents (verified present)

```
mpss-3.8.6-k1om.tar            785.8 MB   <- the card's own OS image
mpss-3.8.6-linux.tar           300.9 MB   <- host-side stack
mpss-src-3.8.6.tar             282.8 MB   <- source, for patching
mpss-downloadcache-3.8.6.tar     1.3 GB
mpss-3.8.6-windows.zip         335.6 MB   <- not needed
```

MPSS 3.8.6 (Oct 2019) is the **last** release. The x100 family was declared end-of-life in
January 2017; the final Intel toolchain to support it was Parallel Studio XE 2017.

## The gap — this is the project

Newest known-good driver: **kernel 4.19**. Host runs **7.0**. Roughly six years of churn in
the DMA mapping, mmap, and PCI APIs sits between them. That delta is the actual work.

## Toolchain caveats

- **ICC is gone.** `-mmic` lived in Intel Parallel Studio XE 2017, which is neither
  distributed nor licensable now. GCC is the only realistic path.
- **The C++ standard is the real gate, not the ISA.** GCC 5.1.1 gives you C++14. SYCL 2020
  requires C++17. Any attempt to run a modern portable-language stack natively on the card
  runs into this before it runs into anything about vectors. See `UNIVERSAL-TRANSLATOR.md`.
- **Verify the IMCI claim on the bench.** The `gcc-5.1.1-knc` README advertises AVX-512
  builtins, but AVX-512 builtins emit **EVEX**, which KNC cannot execute — KNC needs
  **MVEX**. Either the build is patched to re-encode, or the vectorisation story is weaker
  than advertised. Historically, upstream open toolchains gave reliable *scalar* k1om while
  the vector unit was ICC's domain. **Do not plan around IMCI codegen until a
  disassembled object file has been eyeballed.**
- **The card's userland is ancient** — busybox and an old glibc, from the k1om image. Any
  port must be dependency-light or cross-built wholesale.

## Immediate actions

- [ ] Mirror all five MPSS 3.8.6 tarballs to `/srv/knc-archive/mpss-3.8.6/`
- [ ] Clone + archive `subgeniuskitty/xeon-phi-kernel-module`
- [ ] Clone + archive `apc-llc/gcc-5.1.1-knc`
- [ ] Snapshot the `jjkeijser` patch set
- [ ] Build the cross-compiler; compile a trivial kernel; **disassemble and check for MVEX**
