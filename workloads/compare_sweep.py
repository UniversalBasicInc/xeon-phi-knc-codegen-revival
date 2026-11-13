#!/usr/bin/env python3
"""Compare two endomc sweep TSVs (e.g. HQ reference vs Phi run): class agreement cell by cell,
and max relative difference in EA_end / M1_frac / M2_frac. Usage: compare_sweep.py ref.tsv other.tsv"""
import csv, sys, math
def load(p):
    d = {}
    for r in csv.DictReader(open(p), delimiter="\t"):
        d[(r["rho0"], r["log10_omega"], r["log10_beta1"])] = r
    return d
a, b = load(sys.argv[1]), load(sys.argv[2])
keys = sorted(set(a) & set(b)); missing = len(set(a) ^ set(b))
same = sum(1 for k in keys if a[k]["class"] == b[k]["class"])
def rel(x, y):
    x, y = float(x), float(y)
    return abs(x - y) / max(abs(x), abs(y), 1e-300)
worst = {c: max(rel(a[k][c], b[k][c]) for k in keys) for c in ("EA_end", "M1_frac", "M2_frac")}
print(f"cells compared: {len(keys)} (missing on one side: {missing})")
print(f"class agreement: {same}/{len(keys)} = {100*same/len(keys):.3f}%")
for c, w in worst.items(): print(f"max relative difference {c}: {w:.3e}")
diff = [k for k in keys if a[k]["class"] != b[k]["class"]]
for k in diff[:10]: print("  differs:", k, a[k]["class"], "vs", b[k]["class"])
sys.exit(0 if same == len(keys) else 1)
