# Hardware

## The card

Intel Xeon Phi x100 family — **Knights Corner (KNC)**, `k1om` architecture, 2013.
A *coprocessor*, not an accelerator in the modern sense: it boots its own embedded Linux
and you `ssh mic0` into it.

Representative parts (exact SKU TBC once in hand):

| | 3120A | 5110P | 7120P |
|---|---|---|---|
| Cores | 57 | 60 | 61 |
| Threads/core | 4 | 4 | 4 |
| Clock | 1.10 GHz | 1.053 GHz | 1.238 GHz |
| Memory | 6 GB GDDR5 | 8 GB | 16 GB |
| Bandwidth | ~240 GB/s | ~320 GB/s | ~352 GB/s |
| fp64 peak | ~1.0 TF | ~1.01 TF | ~1.21 TF |
| fp32 peak | ~2.0 TF | ~2.02 TF | ~2.42 TF |
| TDP | 300 W | 225 W | 300 W |

**fp64:fp32 = 1:2.** This is the single most important number in the project — see
`WORKLOADS.md`.

**Vector unit: IMCI, 512-bit, MVEX-encoded.** *Not* AVX-512F. See
`UNIVERSAL-TRANSLATOR.md` for why the distinction is both fatal and, oddly, hopeful.

## The host — `xeon-e5`

Verified 2026-08-23:

```
Static hostname: xeon-e5
OS:              Ubuntu 24.04.4 LTS
Kernel:          7.0.0-30-generic   (was -29 at first recon)
CPU:             2 × Intel Xeon E5-2683 v4 @ 2.10GHz — 32C/64T total
Chassis:         Machinist X99 dual (no BMC — openipmi.service fails, expected)
GPU:             RTX 3060 12GB @ 81:00.0, driver 610.43.02 / CUDA UMD 13.3
Root disk:       468G

```

The X99 / E5-26xx v4 platform is **era-correct for KNC**, which matters — this generation
of card is fussy about host BIOS support for large MMIO apertures.

> **Disk note.** Root has ~21G free and the MPSS download set is ~2.7 GB. Stage all
> downloads to `/mnt/backup`, never to `/`.

## Pre-flight checklist — before the card goes in a slot

- [ ] **Above-4G decoding / large BAR enabled in BIOS.** KNC maps a multi-GB aperture and
      will not initialise without it. This is the #1 cause of "card is dead" reports.
- [ ] **Power: 6-pin + 8-pin PCIe.** Budget 225–300 W for the card alone, on top of two
      145 W CPUs and the 3060.
- [ ] **Cooling: the card is PASSIVE.** It is a bare heatsink designed for a rack's
      front-to-back wind tunnel. In a desktop chassis with no forced air it will throttle
      to nothing or shut down. Needs a 3D-printed shroud + a real 40–60mm blower.
- [ ] **A free physical x16 slot** with enough clearance — these are full-length,
      double-width, and heavy.
- [ ] `sudo dmidecode -t slot` — confirm slot inventory (needs sudo; not yet run).
- [ ] **IOMMU / VT-d posture.** Current cmdline sets neither `intel_iommu=on` nor `=off`.
      MPSS is happiest with VT-d off or in passthrough; decide before first boot with
      the card in.

## Chassis caution

Per the fleet notes, this box is currently the PSU/stability experiment. Adding a 300W
passive card to an already-suspect power situation is asking two questions at once.
Resolve the PSU question **first**.
