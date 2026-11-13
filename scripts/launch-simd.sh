#!/bin/bash
# wait for mic0 online in the guest, then copy binaries to the card and launch sweep-simd 101
for i in $(seq 1 180); do
  s=$(ssh -o ConnectTimeout=10 knc-guest 'cat /sys/class/mic/mic0/state' 2>/dev/null)
  echo "$(date +%T) state=$s"
  [ "$s" = online ] && break
  sleep 10
done
[ "$s" = online ] || { echo "card never came online"; exit 1; }
sleep 15
ssh knc-guest 'ssh -o ConnectTimeout=20 root@172.31.1.1 "uname -r; grep -c ^processor /proc/cpuinfo"' || { echo "card ssh not up"; exit 1; }
ssh knc-guest 'scp -q /root/knc-guest/endomc-simd-k1om /root/knc-guest/dosemc-k1om root@172.31.1.1:/tmp/ && ssh root@172.31.1.1 "cd /tmp && cp endomc-simd-k1om endomc && chmod +x endomc dosemc-k1om && md5sum endomc && ./endomc 2>&1 | head -n 3"'
ssh knc-guest 'setsid nohup bash /root/knc-guest/run_on_card.sh /root/knc-guest/endomc-simd-k1om 101 sweep-simd /root/knc-guest/card-results-simd > /root/knc-guest/run-simd.log 2>&1 < /dev/null &'
sleep 20
ssh knc-guest 'cat /root/knc-guest/run-simd.log; ssh root@172.31.1.1 "cat /tmp/results/sweep.log; uptime"'
echo LAUNCHED
