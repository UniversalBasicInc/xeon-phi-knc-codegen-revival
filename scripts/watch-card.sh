#!/bin/bash
# Live watcher for a card sweep: emits every new progress line from the card's sweep.log
# (the solver prints "<done>/<total> in <s>s (<rate> sims/s, ~<eta>s left)" every 1000 sims),
# the card's power/temperature, and ends when DONE appears or the process is gone.
seen=0
while true; do
  r=$(ssh -o ConnectTimeout=15 knc-guest 'ssh -o ConnectTimeout=15 root@172.31.1.1 "cat /tmp/results/sweep.log 2>/dev/null; echo ===; cat /tmp/results/DONE 2>/dev/null; echo ===; ps | grep -c [e]ndomc"; micsmc -t -f 2>/dev/null | grep -E "Cpu Temp|Total Power" | tr -s " " | tr "\n" " "' 2>/dev/null)
  [ -z "$r" ] && { sleep 60; continue; }
  log=$(echo "$r" | sed -n '1,/^===$/p' | grep -v "^===$")
  n=$(echo "$log" | grep -c .)
  if [ "$n" -gt "$seen" ]; then echo "$log" | tail -n $((n - seen)) | grep -E "/|sweep|panic|error" ; seen=$n; echo "$r" | tail -n 1; fi
  done_f=$(echo "$r" | sed -n '/^===$/,/^===$/p' | grep -c "^done")
  nproc=$(echo "$r" | tail -n 2 | head -n 1)
  if [ "$done_f" -gt 0 ] || [ "$nproc" = "0" ]; then echo "SWEEP ENDED done=$done_f running=$nproc"; echo "$log" | tail -n 3; exit 0; fi
  sleep 60
done
