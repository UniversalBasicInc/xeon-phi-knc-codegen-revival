#!/usr/bin/env python3
"""Render the README figures from the result files in this repository.

    python3 workloads/plot_figures.py        # writes docs/figures/*.png

Needs numpy and matplotlib. Every figure is drawn from a checked-in TSV; nothing is
re-simulated here.
"""
import csv
import os

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
from matplotlib.colors import ListedColormap
from matplotlib.patches import Patch

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "docs", "figures")
ENDO = os.path.join(HERE, "endomc", "results")
DOSE = os.path.join(HERE, "dosemc", "results", "xeon-e5-2683v4")

SURFACE, INK, INK2, MUTED, GRID, AXIS = "#fcfcfb", "#0b0b0b", "#52514e", "#898781", "#e1e0d9", "#c3c2b7"
BLUE, ORANGE, AQUA = "#2a78d6", "#eb6834", "#1baf7a"
# ordered classes use one hue, light to dark
CLASS_COLORS = {"Low": "#b7d3f6", "Transient": "#3987e5", "Sustained": "#0d366b"}

plt.rcParams.update({
    "font.family": "DejaVu Sans", "font.size": 10, "text.color": INK,
    "axes.edgecolor": AXIS, "axes.labelcolor": INK2, "axes.titlesize": 11, "axes.titleweight": "bold",
    "axes.titlelocation": "left", "axes.spines.top": False, "axes.spines.right": False,
    "xtick.color": MUTED, "ytick.color": MUTED, "xtick.labelcolor": INK2, "ytick.labelcolor": INK2,
    "grid.color": GRID, "grid.linewidth": 0.6, "figure.facecolor": SURFACE, "axes.facecolor": SURFACE,
    "savefig.facecolor": SURFACE, "savefig.dpi": 160, "legend.frameon": False,
})


def rows(path):
    with open(path, newline="") as f:
        return list(csv.DictReader(f, delimiter="\t"))


def save(fig, name):
    os.makedirs(OUT, exist_ok=True)
    fig.savefig(os.path.join(OUT, name), bbox_inches="tight", pad_inches=0.25)
    plt.close(fig)
    print("wrote docs/figures/" + name)


def fig_class_map():
    """The 30,603-run grid as computed on the Xeon Phi (run v4, identical to the x86 reference)."""
    data = rows(os.path.join(ENDO, "phi-31S1", "sweep-simd-v4-ordered", "sweep.tsv"))
    order = ["Low", "Transient", "Sustained"]
    cmap = ListedColormap([CLASS_COLORS[c] for c in order])
    rhos = sorted({r["rho0"] for r in data}, key=float)
    fig, axes = plt.subplots(1, len(rhos), figsize=(11.5, 4.3), sharey=True)
    for ax, rho in zip(axes, rhos):
        sub = [r for r in data if r["rho0"] == rho]
        xs = sorted({float(r["log10_omega"]) for r in sub})
        ys = sorted({float(r["log10_beta1"]) for r in sub})
        grid = np.full((len(ys), len(xs)), np.nan)
        xi = {v: i for i, v in enumerate(xs)}
        yi = {v: i for i, v in enumerate(ys)}
        for r in sub:
            grid[yi[float(r["log10_beta1"])], xi[float(r["log10_omega"])]] = order.index(r["class"])
        ax.imshow(grid, origin="lower", cmap=cmap, vmin=0, vmax=2, aspect="auto",
                  extent=[xs[0], xs[-1], ys[0], ys[-1]], interpolation="nearest")
        ax.set_title(f"Retrograde influx ρ₀ = {rho}")
        ax.set_xlabel("Immune clearance, log₁₀ ω")
        for s in ax.spines.values():
            s.set_visible(False)
        ax.tick_params(length=0)
        ax.set_xticks([-6, -5.5, -5, -4.5])
    axes[0].set_ylabel("Immune detection, log₁₀ β₁")
    fig.legend(handles=[Patch(facecolor=CLASS_COLORS[c], label=f"{c} attachment") for c in order],
               loc="lower center", ncol=3, bbox_to_anchor=(0.5, -0.06))
    fig.suptitle("Where lesions take hold: 30,603 simulations of the Miller et al. 2025 model, run on a 2013 Xeon Phi",
                 x=0.01, ha="left", fontsize=12, fontweight="bold")
    fig.text(0.01, 0.905, "Each cell is one simulation, classified by attached endometrial cells at cycle ends. "
             "All cells are identical to the x86 reference run.", color=INK2, fontsize=9.5)
    fig.subplots_adjust(top=0.80, wspace=0.10)
    save(fig, "endomc-class-map.png")


def fig_timeseries():
    cases = [("sustained", "Sustained", BLUE), ("default_transient", "Transient (default parameters)", ORANGE),
             ("no_attachment", "No attachment", AQUA)]
    fig, ax = plt.subplots(figsize=(8.6, 4.4))
    floor, t0, t1 = 1e-4, 924, 1092          # the last six 28-day cycles of 39
    for key, label, color in cases:
        d = rows(os.path.join(ENDO, "phi-31S1", "examples", key + "_timeseries.tsv"))
        t = np.array([float(r["t"]) for r in d])
        ea = np.maximum(np.array([float(r["EA"]) for r in d]), floor)
        keep = t >= t0
        ax.plot(t[keep], ea[keep], color=color, linewidth=1.8, label=label)
    ax.set_yscale("log")
    ax.set_ylim(floor, 5e9)
    ax.set_xlim(t0, t1)
    ax.set_xticks(range(t0, t1 + 1, 28))
    ax.set_xlabel("Time (days); one tick per 28-day cycle")
    ax.set_ylabel("Attached endometrial cells (log scale)")
    ax.grid(axis="y")
    ax.set_title("Three outcomes of the same model: attached cells over the last six of 39 cycles", pad=22)
    ax.text(0, 1.03, "Computed on the Xeon Phi 31S1. Values below 10⁻⁴ are drawn at 10⁻⁴.",
            transform=ax.transAxes, color=INK2, fontsize=9.5)
    ax.legend(loc="center left", bbox_to_anchor=(1.01, 0.5))
    save(fig, "endomc-timeseries.png")


def fig_sobol():
    d = rows(os.path.join(ENDO, "xeon-e5-2683v4", "sobol_N512_span3.tsv"))
    d = sorted(d, key=lambda r: float(r["ST_disease"]), reverse=True)[:10][::-1]
    names = [r["parameter"] for r in d]
    st = [float(r["ST_disease"]) for r in d]
    fig, ax = plt.subplots(figsize=(7.6, 4.4))
    bars = ax.barh(names, st, color=BLUE, height=0.62)
    for b, v in zip(bars, st):
        ax.text(v + 0.008, b.get_y() + b.get_height() / 2, f"{v:.2f}", va="center", color=INK2, fontsize=9)
    ax.set_xlim(0, max(st) * 1.14)
    ax.set_xlabel("Total-order Sobol index for the disease outcome")
    ax.grid(axis="x")
    ax.set_axisbelow(True)
    ax.tick_params(axis="y", length=0)
    ax.spines["left"].set_visible(False)
    ax.set_title("Which parameters decide the outcome: top 10 of 27", pad=22)
    ax.text(0, 1.03, "Saltelli design, N = 512, 14,848 simulations, each parameter varied ×/÷ 3 (x86 reference host).",
            transform=ax.transAxes, color=INK2, fontsize=9.5)
    save(fig, "endomc-sobol.png")


def fig_dose():
    def load(name):
        z, p = [], []
        dens = []
        with open(os.path.join(DOSE, name)) as f:
            for line in f:
                if line.startswith("#") or not line.strip():
                    continue
                a = line.split()
                z.append(float(a[0])); dens.append(float(a[1])); p.append(float(a[2]))
        return np.array(z), np.array(p), np.array(dens)
    zw, pw, _ = load("pdd-water-1.25MeV.tsv")
    zl, pl, dl = load("pdd-lung-1.25MeV.tsv")
    lung = zl[dl < 1.0]
    lo, hi = lung.min() - 0.125, lung.max() + 0.125
    fig, ax = plt.subplots(figsize=(8.6, 4.4))
    ax.axvspan(lo, hi, color="#f0efec", zorder=0)
    ax.text((lo + hi) / 2, 104, "lung slab (0.26 g/cm³)", ha="center", color=INK2, fontsize=9)
    ax.plot(zw, pw, color=BLUE, linewidth=1.8, label="Water only")
    ax.plot(zl, pl, color=ORANGE, linewidth=1.8, label="Water / lung / water")
    ax.set_xlim(0, zl.max())
    ax.set_ylim(0, 110)
    ax.set_xlabel("Depth (cm)")
    ax.set_ylabel("Percent depth dose")
    ax.grid(axis="y")
    ax.legend(loc="lower left")
    ax.set_title("Monte Carlo photon dose through a lung slab, 1.25 MeV, 4 million histories", pad=22)
    ax.text(0, 1.03, "Simplified physics, cross-section tables not yet re-verified against NIST: qualitative only (x86 reference host).",
            transform=ax.transAxes, color=INK2, fontsize=9.5)
    save(fig, "dosemc-lung-slab.png")


if __name__ == "__main__":
    fig_class_map()
    fig_timeseries()
    fig_sobol()
    fig_dose()
