# Startup — the EPYC host + Xeon Phi 31S1 + Debian 10 guest `knc-guest`

Verified 2026-11-02. Card: Xeon Phi 31S1 (8086:225e), the EPYC host CPU SLOT6, `0000:01:00.0`.
Guest: libvirt domain `knc-guest` (Debian 10, kernel 4.19.0-27, MPSS 3.8.6, patched mic.ko).
Reach the guest from HQ with `ssh knc-guest` (alias in `~/.ssh/config`, ProxyJump via the EPYC host).

## The one rule

**Never let anything hot-reset the card.** A PCIe bus reset (what vfio-pci does by
default when a guest opens or releases the device) leaves the card stuck at POST code
`30` ("Begin memory training") until the next *full power cycle*. A host reboot is not
enough. The driver's own software reset (SBOX RGCR) is fine.

## After every the EPYC host boot, before starting the guest

Run on the EPYC host as root (copy of `scripts/phi-attach.sh` lives at `/data/knc/phi-attach.sh`):

```sh
/data/knc/phi-attach.sh
```

It finds the card by vendor:device (**the PCI address changes when other cards are added or
removed** — on 2026-11-02 it moved off `0000:01:00.0` when the GPUs came out, and the NVMe took
that address), rewrites the guest's `<hostdev>` to match, then does the three steps by hand
if you prefer:

```sh
B=$(lspci -D -d 8086:225e | cut -d' ' -f1)
echo > /sys/bus/pci/devices/$B/reset_method          # no reset methods at all
echo vfio-pci > /sys/bus/pci/devices/$B/driver_override && echo $B > /sys/bus/pci/drivers_probe
virsh start knc-guest                                      # only after the hostdev address matches $B
```

- Line 1 empties the card's reset-method list (kernel >= 5.15), so vfio-pci has nothing
  to reset with. Check: `cat /sys/bus/pci/devices/0000:01:00.0/reset_method` prints
  nothing.
- Line 2 binds the card to vfio-pci (the `vfio-pci` module loads at boot via
  `/etc/modules-load.d/vfio-phi.conf`; the old `modprobe.d` ids file is parked at
  `/data/knc/vfio-phi.conf.disabled` on purpose — it would bind before the
  reset method is cleared).
- Line 3 starts the guest. `virsh reset` / `virsh destroy` are safe once the reset
  method is cleared.

Fans: the board idles the headers. Force full speed before booting the card:

```sh
ipmitool raw 0x30 0x45 0x01 0x01     # Supermicro: fan mode = full
ipmitool sdr type fan
```

## Check the card without any driver (host side)

Only works while the card is **not** bound to vfio-pci (it maps BAR4 exclusively):

```sh
python3 /data/knc/postcode.py      # POST 12 = ready (good); 30 = needs a power cycle
```

## Inside the guest

```sh
cat /sys/class/mic/mic0/state /sys/class/mic/mic0/post_code   # expect: ready / 12
/root/knc-guest/03-boot-card.sh        # boots the card OS, captures the oracle into /root/knc-guest/oracle/
micctrl -s; micinfo; micsmc        # status, inventory, temperatures (card must be online)
```

Scripts in the guest, in order: `01-build-driver.sh`, `02-install-mpss.sh`,
`03-boot-card.sh`, `04-rebuild-driver.sh`. Driver source with the two port fixes
(sched/mm.h include; mod_timer after every timer_setup) in
`/root/knc-guest/xeon-phi-kernel-module`.

## Files on the EPYC host

`/data/knc/{vm,mpss-3.8.6,repos,logs,mkvm.sh,postcode.py,users-guide.txt}`
(users-guide.txt is pdftotext of MPSS_Users_Guide.pdf; POST code table near line 11348).

## Slot: CPU SLOT6 only (2026-11-03)

In CPU SLOT7 (bus 81, root port 80:07.1) the card enumerates with a clean Gen2 x16 link but the
bootstrap hangs at POST 30 from power-on, every cold boot, with PLL locked and die warm; no reset
recovers it. Moved back to SLOT6 (bus 01, root 00:07.1 side) it reads POST 12 at power-on. Slot 7
is not a known-bad board slot in general, just not for this card; do not move it again.

## If `phi-attach.sh` says "no Xeon Phi on the PCI bus" (seen 2026-11-03)

The BIOS enumerates the card at power-on; a KNC that is seated but has no 6-pin + 8-pin aux
power, or is not fully seated, never trains its link and is simply absent (`dmidecode -t slot`
shows every CPU SLOT "Available"). `echo 1 > /sys/bus/pci/rescan` does not help. Fix = full
power off, check both aux plugs and seating in CPU SLOT6, power on, re-run `phi-attach.sh`.

The libvirt domain keeps the LAST address the script wrote. After the GPUs came out, that
address (01:00.0) became the root NVMe, so the hostdev is now `managed='no'`: a bare
`virsh start knc-guest` fails instead of detaching the root disk. Always start via `phi-attach.sh`,
which rewrites the address and binds vfio-pci itself.
