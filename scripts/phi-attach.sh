#!/bin/bash
# Run on the EPYC host as root after every host boot, before using the guest.
# Finds the Phi wherever the bus numbering put it (it moves when slots change), points the
# libvirt domain at it, disables PCI resets (a hot reset strands KNC at POST 30), binds vfio-pci,
# and starts the guest. The domain hostdev is managed='no' on purpose: a bare `virsh start`
# must fail rather than detach whatever now lives at the last address.
set -euo pipefail
DOM=${DOM:-knc-guest}
B=$(lspci -D -d 8086:225e | cut -d' ' -f1)
[ -n "$B" ] || { echo "no Xeon Phi (8086:225e) on the PCI bus" >&2; exit 1; }
echo "Phi at $B"
virsh dumpxml "$DOM" > /tmp/$DOM.xml
python3 - "$B" /tmp/$DOM.xml <<'PY'
import re, sys
bdf, path = sys.argv[1:]
d, b, sf = bdf.split(":"); s, f = sf.split(".")
x = open(path).read()
pat = re.compile(r"(<hostdev[^>]*>\s*<source>\s*<address) domain=['\"]0x[0-9a-f]+['\"] bus=['\"]0x[0-9a-f]+['\"] slot=['\"]0x[0-9a-f]+['\"] function=['\"]0x[0-9a-f]+['\"]")
rep = lambda m: f"{m.group(1)} domain='0x{int(d,16):04x}' bus='0x{int(b,16):02x}' slot='0x{int(s,16):02x}' function='0x{int(f,16)}'"
new, n = pat.subn(rep, x, count=1)
assert n == 1, "hostdev address not found in domain XML"
open(path, "w").write(new)
print("domain hostdev:", "updated" if new != x else "unchanged")
PY
virsh define /tmp/$DOM.xml >/dev/null
echo > /sys/bus/pci/devices/$B/reset_method
echo "reset_method: [$(cat /sys/bus/pci/devices/$B/reset_method)]"
modprobe vfio-pci
if [ "$(basename "$(readlink -f /sys/bus/pci/devices/$B/driver 2>/dev/null || echo none)")" != vfio-pci ]; then
  echo vfio-pci > /sys/bus/pci/devices/$B/driver_override
  echo "$B" > /sys/bus/pci/drivers_probe
fi
echo "driver: $(basename "$(readlink /sys/bus/pci/devices/$B/driver)")"
ipmitool raw 0x30 0x45 0x01 0x01 >/dev/null 2>&1 && echo "fans: full"
virsh start "$DOM"
