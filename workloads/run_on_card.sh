#!/bin/bash
# Run from the Debian 10 guest (knc-guest) once a k1om binary exists. Copies it to the card, runs the
# endomc sweep on all 228 threads with temperature/power sampled every 10 s, pulls the results back.
# Usage: run_on_card.sh /path/to/endomc-k1om [n_samples] [mode: sweep|sweep-simd] [out_dir]
set -euo pipefail
BIN=$1; N=${2:-101}; MODE=${3:-sweep}; CARD=root@172.31.1.1; OUT=${4:-/root/knc-guest/card-results}; mkdir -p $OUT
scp -q "$BIN" $CARD:/tmp/endomc
# subshell + stdin from /dev/null, or the card's sshd keeps the session open and this line never returns
ssh $CARD 'mkdir -p /tmp/results && rm -f /tmp/results/DONE && (nohup sh -c "cd /tmp && ./endomc '"$MODE"' '"$N"' /tmp/results/sweep.tsv > /tmp/results/sweep.log 2>&1; echo done > /tmp/results/DONE" >/dev/null 2>&1 </dev/null &)'
: > $OUT/thermal.log
while ! ssh $CARD test -f /tmp/results/DONE; do
  echo "$(date +%T) $(micsmc -t -f 2>/dev/null | grep -E 'Cpu Temp|Total Power' | tr -s ' ' | tr '\n' ' ')" | tee -a $OUT/thermal.log
  sleep 10
done
scp -q $CARD:/tmp/results/sweep.tsv $CARD:/tmp/results/sweep.log $OUT/
tail -1 $OUT/sweep.log
echo "results in $OUT; compare with: python3 compare_sweep.py <hq sweep tsv> $OUT/sweep.tsv"
