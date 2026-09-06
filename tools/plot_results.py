#!/usr/bin/env python3
"""Generate report figures from definitive batch JSON exports.

Inputs (produced by the definitive /sim/batch run):
  outputs/definitive_results.json
  outputs/definitive_hypothesis.json

Outputs:
  report/figures/kpi_boxplots.png
  report/figures/hypothesis_deltas.png

Requires: matplotlib (stdlib otherwise).
"""
from __future__ import annotations

import json
import os
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RESULTS = ROOT / "outputs" / "definitive_results.json"
HYP = ROOT / "outputs" / "definitive_hypothesis.json"
OUT = ROOT / "report" / "figures"


def main() -> None:
    try:
        import matplotlib

        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
    except ImportError as e:
        raise SystemExit("matplotlib required: pip install matplotlib") from e

    rows = json.loads(RESULTS.read_text())
    hyp = json.loads(HYP.read_text())
    OUT.mkdir(parents=True, exist_ok=True)

    scenarios = ["CalmPassage", "StormCorridor", "BlindShore", "DeepWaterRescue"]
    methods = ["BaselineA", "BaselineB", "ProposedSystem"]
    method_labels = {"BaselineA": "A", "BaselineB": "B", "ProposedSystem": "P"}
    colors = {"BaselineA": "#64748b", "BaselineB": "#f59e0b", "ProposedSystem": "#0ea5e9"}

    # ── KPI box plots (collision + fatal) ────────────────────────────────────
    fig, axes = plt.subplots(2, 2, figsize=(10, 7), sharex=False)
    kpis = [
        ("collision_per_1k_hrs", "Collisions / 1k ship-hrs"),
        ("fatal_per_1k_hrs", "Fatals / 1k ship-hrs"),
        ("survival_ratio", "Survival ratio"),
        ("mean_p_prep", "Mean P_prep"),
    ]
    by = defaultdict(list)
    for r in rows:
        for k, _ in kpis:
            by[(r["scenario"], r["method"], k)].append(r["kpi"][k])

    for ax, (kpi, title) in zip(axes.flat, kpis):
        positions = []
        data = []
        tick_pos = []
        tick_lab = []
        pos = 1.0
        for si, sc in enumerate(scenarios):
            group_start = pos
            for m in methods:
                data.append(by[(sc, m, kpi)])
                positions.append(pos)
                ax.boxplot(
                    [by[(sc, m, kpi)]],
                    positions=[pos],
                    widths=0.6,
                    patch_artist=True,
                    boxprops=dict(facecolor=colors[m], alpha=0.7),
                    medianprops=dict(color="black"),
                    flierprops=dict(marker=".", markersize=3),
                )
                pos += 1.0
            tick_pos.append((group_start + pos - 1) / 2)
            tick_lab.append(sc.replace("Passage", "").replace("Corridor", "").replace("Rescue", ""))
            pos += 0.8
        ax.set_xticks(tick_pos)
        ax.set_xticklabels(tick_lab, fontsize=8)
        ax.set_title(title, fontsize=10)
        ax.grid(True, axis="y", alpha=0.3)
    # legend
    handles = [
        plt.matplotlib.patches.Patch(color=colors[m], label=method_labels[m], alpha=0.7)
        for m in methods
    ]
    fig.legend(handles=handles, loc="upper center", ncol=3, frameon=False)
    fig.suptitle("Definitive batch KPIs (30 seeds × 4 scenarios × 3 methods)", y=0.98)
    fig.tight_layout(rect=[0, 0, 1, 0.94])
    fig.savefig(OUT / "kpi_boxplots.png", dpi=160)
    plt.close(fig)
    print(f"Wrote {OUT / 'kpi_boxplots.png'}")

    # ── Hypothesis δ bars for H1–H3 relevant tests ───────────────────────────
    focus = []
    for h in hyp:
        if h["method_b"] != "ProposedSystem":
            continue
        if h["kpi"] == "fatal_per_1k_hrs" and h["method_a"] in ("BaselineA", "BaselineB"):
            focus.append(h)
        elif h["kpi"] == "survival_ratio" and h["method_a"] == "BaselineA":
            focus.append(h)

    fig, ax = plt.subplots(figsize=(10, 5))
    labels = []
    deltas = []
    bar_colors = []
    for h in sorted(focus, key=lambda x: (x["kpi"], x["scenario"], x["method_a"])):
        tag = "H1" if h["kpi"] == "fatal_per_1k_hrs" and h["method_a"] == "BaselineA" else (
            "H3" if h["kpi"] == "fatal_per_1k_hrs" else "H2"
        )
        labels.append(f"{tag}\n{h['scenario'][:6]}\n{h['method_a'][-1]}→P")
        # signed δ: positive means method_a > method_b on the KPI
        deltas.append(h["cliffs_delta"])
        if h["confirmed"]:
            bar_colors.append("#16a34a")
        elif h["significant"]:
            bar_colors.append("#ca8a04")
        else:
            bar_colors.append("#94a3b8")
    ax.bar(range(len(deltas)), deltas, color=bar_colors)
    ax.axhline(0.2, color="#16a34a", ls="--", lw=0.8, label="|δ|=0.2")
    ax.axhline(-0.2, color="#16a34a", ls="--", lw=0.8)
    ax.axhline(0, color="black", lw=0.5)
    ax.set_xticks(range(len(labels)))
    ax.set_xticklabels(labels, fontsize=7)
    ax.set_ylabel("Cliff's δ (method_a − Proposed)")
    ax.set_title("Hypothesis effect sizes (green=confirmed, amber=sig only, grey=n.s.)")
    ax.grid(True, axis="y", alpha=0.3)
    fig.tight_layout()
    fig.savefig(OUT / "hypothesis_deltas.png", dpi=160)
    plt.close(fig)
    print(f"Wrote {OUT / 'hypothesis_deltas.png'}")


if __name__ == "__main__":
    main()
