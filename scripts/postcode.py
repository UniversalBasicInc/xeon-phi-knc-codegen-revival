#!/usr/bin/env python3
import subprocess
# Read the Xeon Phi (KNC) bootstrap POST code straight from BAR4 on the host, without any driver.
# DBOX base 0x0 + MICREG_POSTCODE 0x242c (from MPSS mic_common.h); low two bytes are ASCII.
import mmap, os, sys, struct
dev = sys.argv[1] if len(sys.argv) > 1 else subprocess.run(["lspci","-D","-d","8086:225e"],capture_output=True,text=True).stdout.split()[0]
with open(f"/sys/bus/pci/devices/{dev}/resource4", "r+b") as f:
    m = mmap.mmap(f.fileno(), 0x20000, mmap.MAP_SHARED, mmap.PROT_READ)
    v = struct.unpack("<I", m[0x242c:0x2430])[0]
    m.close()
code = bytes([v & 0xff, (v >> 8) & 0xff])
print(f"raw=0x{v:08x} post_code={code.decode(errors='replace')!r}")
