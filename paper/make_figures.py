#!/usr/bin/env python3
"""Genera las figuras de convergencia del paper a partir de los CSV del
Convergence Lab (paper/data/*.csv). Sin dependencias exóticas: numpy + matplotlib.

    python3 paper/make_figures.py

Produce paper/figures/*.png y imprime la tabla de pendientes ajustadas.
"""
import csv
import glob
import os
import numpy as np
import matplotlib.pyplot as plt

HERE = os.path.dirname(os.path.abspath(__file__))
DATA = os.path.join(HERE, "data")
FIGS = os.path.join(HERE, "figures")
os.makedirs(FIGS, exist_ok=True)

# Estilo coherente con el instrumento (tema RED808 sobre fondo claro para impresión)
SERIES = [
    ("random",     "#d62728", "Random",     "o"),
    ("stratified", "#2ca02c", "Stratified", "s"),
    ("qmc",        "#1f77b4", "Quasi-MC",   "^"),
    ("importance", "#bf9000", "Importance", "D"),
    ("reverse",    "#9467bd", "Reverse",    "v"),
]


def load(path):
    meta = {}
    with open(path) as f:
        rows = list(csv.reader(f))
    for r in rows:
        if r and r[0].startswith("#"):
            line = r[0].lstrip("# ").strip() + ("," + ",".join(r[1:]) if len(r) > 1 else "")
            if "aperture_ms=" in line:
                for kv in line.split(","):
                    if "=" in kv:
                        k, v = kv.split("=", 1)
                        meta[k.strip()] = v.strip()
            if line.startswith("slopes:"):
                for kv in line.replace("slopes:", "").split(","):
                    if "=" in kv:
                        k, v = kv.split("=")
                        meta["slope_" + k.strip()] = float(v)
    # tabla numérica
    hdr_i = next(i for i, r in enumerate(rows) if r and r[0] == "N")
    header = rows[hdr_i]
    data = np.array([[float(x) for x in r] for r in rows[hdr_i + 1:] if r], dtype=float)
    cols = {name: data[:, header.index(name)] for name in header}
    return meta, cols


def fit_slope(N, y):
    m = y > 0
    return np.polyfit(np.log10(N[m]), np.log10(y[m]), 1)[0]


def plot_run(meta, cols, title, out):
    N = cols["N"]
    fig, ax = plt.subplots(figsize=(6.2, 4.4))
    # referencia ideal 1/sqrt(N), anclada al primer punto de random
    anchor = cols["random"][0]
    ax.plot(N, anchor / np.sqrt(N / N[0]), "--", color="0.6", lw=1.2,
            label=r"ideal $1/\sqrt{N}$ (slope $-0.5$)", zorder=1)
    for key, color, label, mk in SERIES:
        y = cols[key]
        s = fit_slope(N, y)
        ax.plot(N, y, marker=mk, ms=4, lw=1.6, color=color,
                label=f"{label}  ({s:+.2f})")
    ax.set_xscale("log", base=2)
    ax.set_yscale("log")
    ax.set_xlabel("N  (rays / grains per estimate)")
    ax.set_ylabel("RMS error vs deterministic target")
    ax.set_title(title, fontsize=10)
    ax.grid(True, which="both", alpha=0.25)
    ax.legend(fontsize=7.5, framealpha=0.9, loc="lower left")
    fig.tight_layout()
    fig.savefig(out, dpi=160)
    plt.close(fig)


def main():
    files = sorted(glob.glob(os.path.join(DATA, "*.csv")))
    table = []
    runs = []
    for p in files:
        meta, cols = load(p)
        runs.append((os.path.basename(p), meta, cols))
        ap = meta.get("aperture_ms", "?")
        foc = meta.get("focus_s", "?")
        title = f"Convergence — aperture {ap} ms, focus {foc} s"
        out = os.path.join(FIGS, os.path.basename(p).replace(".csv", ".png"))
        plot_run(meta, cols, title, out)
        row = [os.path.basename(p), ap, foc] + [fit_slope(cols["N"], cols[k[0]])
                            for k in SERIES]
        table.append(row)

    # Figura resumen: todas las curvas, mostrando consistencia. El grid se
    # dimensiona al nº de runs (antes era 2×2 fijo y un 5º run se caía en
    # silencio del resumen).
    ncols = 2
    nrows = (len(runs) + ncols - 1) // ncols
    fig, axes = plt.subplots(nrows, ncols, figsize=(9.5, 3.5 * nrows))
    axes = np.atleast_2d(axes)
    for ax in axes.ravel()[len(runs):]:
        ax.set_visible(False)  # celdas sobrantes del grid
    for ax, (name, meta, cols) in zip(axes.ravel(), runs):
        N = cols["N"]
        anchor = cols["random"][0]
        ax.plot(N, anchor / np.sqrt(N / N[0]), "--", color="0.6", lw=1)
        for key, color, label, mk in SERIES:
            ax.plot(N, cols[key], marker=mk, ms=3, lw=1.3, color=color, label=label)
        ax.set_xscale("log", base=2); ax.set_yscale("log")
        ax.set_title(f"ap {meta.get('aperture_ms','?')} ms · focus {meta.get('focus_s','?')} s",
                     fontsize=8.5)
        ax.grid(True, which="both", alpha=0.2)
    axes[0, 0].legend(fontsize=7, ncol=2)
    fig.suptitle(f"RayDrone — Monte Carlo convergence ({len(runs)} runs)", fontsize=11)
    fig.tight_layout(rect=(0, 0, 1, 0.97))
    fig.savefig(os.path.join(FIGS, "summary-grid.png"), dpi=150)
    plt.close(fig)

    # Tabla de pendientes
    print("\nFitted convergence slopes (log RMS vs log N):\n")
    hdr = ["run", "ap_ms", "focus_s"] + [s[0] for s in SERIES]
    print("  ".join(f"{h:>12}" for h in hdr))
    for row in table:
        cells = [str(row[0]), str(row[1]), str(row[2])] + [f"{v:+.3f}" for v in row[3:]]
        print("  ".join(f"{c:>12}" for c in cells))
    # promedios
    real_rows = [r for r in table if not str(r[0]).startswith("run5-")]
    arr = np.array([[r[i] for r in real_rows] for i in range(3, 3 + len(SERIES))], dtype=float)
    means = arr.mean(axis=1)
    print("  ".join(f"{c:>12}" for c in ["MEAN REAL", "", ""] + [f"{m:+.3f}" for m in means]))
    print(f"\nFiguras escritas en {FIGS}")


if __name__ == "__main__":
    main()
