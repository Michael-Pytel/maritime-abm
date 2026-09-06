#!/usr/bin/env python3
"""Generate publication figures (seaborn) from all experiment artifacts.

Inputs
------
  outputs/definitive_results.json
  outputs/definitive_hypothesis.json
  outputs/results.db                 (Tier-1 storm grid)
  outputs/tier2_sweep_summary.csv
  outputs/promote_30seed_ranked.csv  (optional annotation)

Outputs (report/figures/)
-------------------------
  kpi_distributions.png
  preparedness_collisions.png
  hypothesis_heatmap.png
  storm_sweep_heatmap.png
  route_density.png
  hypothesis_deltas.png   (legacy alias of hypothesis_heatmap)
  kpi_boxplots.png        (legacy alias of kpi_distributions)
"""
from __future__ import annotations

import json
import sqlite3
from collections import defaultdict
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
import pandas as pd
import seaborn as sns

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "report" / "figures"
RESULTS = ROOT / "outputs" / "definitive_results.json"
HYP = ROOT / "outputs" / "definitive_hypothesis.json"
DB = ROOT / "outputs" / "results.db"
TIER2 = ROOT / "outputs" / "tier2_sweep_summary.csv"

SCENARIO_LABEL = {
    "CalmPassage": "Calm",
    "StormCorridor": "Storm",
    "BlindShore": "Blind",
    "DeepWaterRescue": "Deep",
}
METHOD_LABEL = {
    "BaselineA": "A",
    "BaselineB": "B",
    "ProposedSystem": "P",
}
METHOD_ORDER = ["BaselineA", "BaselineB", "ProposedSystem"]
SCENARIO_ORDER = ["CalmPassage", "StormCorridor", "BlindShore", "DeepWaterRescue"]

# Academic, non-slop palette: slate / amber / teal
PALETTE = {
    "BaselineA": "#475569",
    "BaselineB": "#d97706",
    "ProposedSystem": "#0f766e",
}


def style() -> None:
    sns.set_theme(
        context="paper",
        style="whitegrid",
        font="DejaVu Sans",
        rc={
            "axes.titlesize": 10,
            "axes.labelsize": 9,
            "xtick.labelsize": 8,
            "ytick.labelsize": 8,
            "legend.fontsize": 8,
            "figure.dpi": 150,
            "savefig.dpi": 200,
            "axes.spines.top": False,
            "axes.spines.right": False,
        },
    )


def load_definitive() -> pd.DataFrame:
    rows = json.loads(RESULTS.read_text())
    recs = []
    for r in rows:
        rec = {
            "scenario": r["scenario"],
            "method": r["method"],
            "seed": r["seed"],
            "iwrap_nc_per_year": r.get("iwrap_nc_per_year", 0.0),
            **r["kpi"],
        }
        recs.append(rec)
    df = pd.DataFrame(recs)
    df["scenario_lab"] = df["scenario"].map(SCENARIO_LABEL)
    df["method_lab"] = df["method"].map(METHOD_LABEL)
    return df


def load_hypothesis() -> pd.DataFrame:
    return pd.DataFrame(json.loads(HYP.read_text()))


def load_storm_grid() -> pd.DataFrame:
    """Rebuild Tier-1 scout means (5 seeds) from SQLite."""
    con = sqlite3.connect(DB)
    rows = con.execute(
        """
        SELECT params, scenario, method, seed,
               collision_per_1k_hrs, fatal_per_1k_hrs, mean_p_prep
        FROM runs
        WHERE params LIKE '%storm_comms_success_rate%'
          AND params NOT LIKE '%ais_path%'
        """
    ).fetchall()
    con.close()
    recs = []
    for params, sc, m, seed, coll, fatal, prep in rows:
        p = json.loads(params)
        # Prefer the original 5-seed scout; for the promoted cell keep seeds 1..5
        # so the grid is comparable across points.
        if int(seed) > 5:
            continue
        recs.append(
            {
                "storm_comms": p["storm_comms_success_rate"],
                "storm_radius": p["storm_radius_nm"],
                "scenario": sc,
                "method": m,
                "seed": seed,
                "collision_per_1k_hrs": coll,
                "fatal_per_1k_hrs": fatal,
                "mean_p_prep": prep,
            }
        )
    return pd.DataFrame(recs)


def fig_kpi_distributions(df: pd.DataFrame) -> None:
    kpis = [
        ("collision_per_1k_hrs", "Collisions / 1k ship-hrs"),
        ("fatal_per_1k_hrs", "Fatals / 1k ship-hrs"),
        ("survival_ratio", "Survival ratio"),
        ("mean_p_prep", "Mean $P_{\\mathrm{prep}}$"),
    ]
    fig, axes = plt.subplots(2, 2, figsize=(10.5, 7.2), constrained_layout=True)
    for ax, (col, title) in zip(axes.flat, kpis):
        sns.boxplot(
            data=df,
            x="scenario_lab",
            y=col,
            hue="method",
            hue_order=METHOD_ORDER,
            palette=PALETTE,
            ax=ax,
            fliersize=2,
            linewidth=0.8,
            width=0.7,
        )
        ax.set_title(title)
        ax.set_xlabel("")
        ax.set_ylabel("")
        if ax is not axes.flat[0]:
            leg = ax.get_legend()
            if leg:
                leg.remove()
        else:
            handles, _ = ax.get_legend_handles_labels()
            ax.legend(
                handles,
                ["Baseline A", "Baseline B", "Proposed"],
                title=None,
                frameon=False,
                loc="upper right",
            )
    fig.suptitle("Definitive factorial ($4{\\times}3{\\times}30$ seeds)", fontsize=11)
    path = OUT / "kpi_distributions.png"
    fig.savefig(path, bbox_inches="tight")
    fig.savefig(OUT / "kpi_boxplots.png", bbox_inches="tight")  # legacy name
    plt.close(fig)
    print("Wrote", path)


def fig_prep_collisions(df: pd.DataFrame) -> None:
    fig, axes = plt.subplots(1, 2, figsize=(10.5, 4.0), constrained_layout=True)

    sns.violinplot(
        data=df,
        x="scenario_lab",
        y="mean_p_prep",
        hue="method",
        hue_order=METHOD_ORDER,
        palette=PALETTE,
        ax=axes[0],
        cut=0,
        inner="quartile",
        linewidth=0.7,
    )
    axes[0].set_title("Crew preparedness")
    axes[0].set_xlabel("")
    axes[0].set_ylabel(r"$P_{\mathrm{prep}}$")
    axes[0].legend(
        title=None,
        labels=["A", "B", "P"],
        frameon=False,
        loc="lower left",
    )

    storm = df[df["scenario"] == "StormCorridor"]
    sns.stripplot(
        data=storm,
        x="method_lab",
        y="collision_per_1k_hrs",
        hue="method",
        hue_order=METHOD_ORDER,
        palette=PALETTE,
        ax=axes[1],
        dodge=False,
        alpha=0.55,
        size=4,
        legend=False,
        order=["A", "B", "P"],
    )
    sns.pointplot(
        data=storm,
        x="method_lab",
        y="collision_per_1k_hrs",
        order=["A", "B", "P"],
        color="#111827",
        ax=axes[1],
        errorbar=("ci", 95),
        markers="D",
        markersize=5,
    )
    axes[1].set_title("Storm Corridor collisions")
    axes[1].set_xlabel("Method")
    axes[1].set_ylabel("Collisions / 1k ship-hrs")

    path = OUT / "preparedness_collisions.png"
    fig.savefig(path, bbox_inches="tight")
    plt.close(fig)
    print("Wrote", path)


def fig_hypothesis_heatmap(hyp: pd.DataFrame) -> None:
    # Focus on Proposed vs A for core KPIs across scenarios
    focus = hyp[
        (hyp["method_a"] == "BaselineA")
        & (hyp["method_b"] == "ProposedSystem")
        & (hyp["kpi"].isin(
            [
                "fatal_per_1k_hrs",
                "survival_ratio",
                "collision_per_1k_hrs",
                "mean_p_prep",
            ]
        ))
    ].copy()
    kpi_lab = {
        "fatal_per_1k_hrs": "Fatal rate (H1)",
        "survival_ratio": "Survival (H2)",
        "collision_per_1k_hrs": "Collisions",
        "mean_p_prep": r"$P_{\mathrm{prep}}$",
    }
    focus["kpi_lab"] = focus["kpi"].map(kpi_lab)
    focus["scenario_lab"] = focus["scenario"].map(SCENARIO_LABEL)
    # Cliff's δ: positive ⇒ A > Proposed on the KPI scale as stored
    mat = focus.pivot(index="kpi_lab", columns="scenario_lab", values="cliffs_delta")
    mat = mat.reindex(columns=[SCENARIO_LABEL[s] for s in SCENARIO_ORDER])
    mat = mat.reindex(
        index=["Fatal rate (H1)", "Survival (H2)", "Collisions", r"$P_{\mathrm{prep}}$"]
    )

    conf = focus.pivot(index="kpi_lab", columns="scenario_lab", values="confirmed")
    conf = conf.reindex(index=mat.index, columns=mat.columns)

    fig, ax = plt.subplots(figsize=(7.2, 3.6), constrained_layout=True)
    sns.heatmap(
        mat,
        ax=ax,
        cmap="RdBu_r",
        center=0,
        annot=True,
        fmt=".2f",
        linewidths=0.6,
        linecolor="white",
        cbar_kws={"label": "Cliff's $\\delta$ (A vs Proposed)"},
        vmin=-1,
        vmax=1,
    )
    # Mark confirmed cells
    for i, kpi in enumerate(mat.index):
        for j, sc in enumerate(mat.columns):
            if bool(conf.loc[kpi, sc]):
                ax.add_patch(
                    plt.Rectangle(
                        (j, i), 1, 1, fill=False, edgecolor="#064e3b", lw=2.0
                    )
                )
    ax.set_xlabel("")
    ax.set_ylabel("")
    ax.set_title("Effect sizes: Baseline A vs Proposed (box = Holm-confirmed)")
    path = OUT / "hypothesis_heatmap.png"
    fig.savefig(path, bbox_inches="tight")
    fig.savefig(OUT / "hypothesis_deltas.png", bbox_inches="tight")
    plt.close(fig)
    print("Wrote", path)


def fig_storm_sweep(grid: pd.DataFrame) -> None:
    a = grid[(grid["method"] == "BaselineA")]
    calm = (
        a[a["scenario"] == "CalmPassage"]
        .groupby(["storm_comms", "storm_radius"], as_index=False)["collision_per_1k_hrs"]
        .mean()
        .rename(columns={"collision_per_1k_hrs": "calm"})
    )
    storm = (
        a[a["scenario"] == "StormCorridor"]
        .groupby(["storm_comms", "storm_radius"], as_index=False)["collision_per_1k_hrs"]
        .mean()
        .rename(columns={"collision_per_1k_hrs": "storm"})
    )
    m = calm.merge(storm, on=["storm_comms", "storm_radius"])
    m["ratio"] = m["storm"] / m["calm"]

    fig, axes = plt.subplots(1, 2, figsize=(10.5, 3.8), constrained_layout=True)
    for ax, col, title, fmt, cmap, center in [
        (
            axes[0],
            "calm",
            "Calm Passage collisions (Baseline A)",
            ".2f",
            "YlOrBr",
            None,
        ),
        (
            axes[1],
            "ratio",
            "Storm / Calm collision ratio (Baseline A)",
            ".2f",
            "coolwarm",
            2.7,
        ),
    ]:
        pivot = m.pivot(index="storm_comms", columns="storm_radius", values=col)
        pivot = pivot.reindex(index=sorted(pivot.index), columns=sorted(pivot.columns))
        sns.heatmap(
            pivot,
            ax=ax,
            annot=True,
            fmt=fmt,
            cmap=cmap,
            center=center,
            linewidths=0.6,
            linecolor="white",
            cbar_kws={"shrink": 0.85},
        )
        ax.set_xlabel("Storm radius (nm)")
        ax.set_ylabel("Storm comms success rate")
        ax.set_title(title)

    path = OUT / "storm_sweep_heatmap.png"
    fig.savefig(path, bbox_inches="tight")
    plt.close(fig)
    print("Wrote", path)


def fig_route_density() -> None:
    if not TIER2.exists():
        print("Skip route density: missing", TIER2)
        return
    df = pd.read_csv(TIER2)
    df["network"] = df["ais_path"].map(
        lambda p: Path(p).stem.replace("routes_", "")
    )
    df["scenario_lab"] = df["scenario"].map(SCENARIO_LABEL)
    order = ["sparse", "baseline", "dense"]
    df = df[df["network"].isin(order)]

    # Compact two-panel figure sized for a single Interspeech column;
    # legend sits inside the Calm panel (empty upper area), not outside.
    g = sns.catplot(
        data=df,
        kind="bar",
        x="network",
        y="collision_per_1k_hrs",
        hue="method",
        col="scenario_lab",
        hue_order=METHOD_ORDER,
        palette=PALETTE,
        order=order,
        height=2.55,
        aspect=0.78,
        legend=False,
        errorbar=None,
    )
    g.set_axis_labels("Route network", "Collisions / 1k ship-hrs")
    g.set_titles("{col_name}")
    for ax in g.axes.flat:
        ax.tick_params(axis="x", labelsize=7)
        ax.tick_params(axis="y", labelsize=7)
    handles = [
        plt.Rectangle((0, 0), 1, 1, color=PALETTE[m])
        for m in METHOD_ORDER
    ]
    g.axes.flat[0].legend(
        handles,
        ["Baseline A", "Baseline B", "Proposed"],
        loc="upper left",
        frameon=True,
        fancybox=False,
        edgecolor="#d1d5db",
        fontsize=6.5,
        handlelength=1.0,
        handletextpad=0.4,
        borderpad=0.35,
        labelspacing=0.25,
    )
    g.fig.suptitle("Tier-2 route-density (5 seeds)", y=1.02, fontsize=9)
    g.fig.set_size_inches(3.35, 2.7)
    path = OUT / "route_density.png"
    g.savefig(path, bbox_inches="tight", dpi=220)
    plt.close(g.fig)
    print("Wrote", path)


def write_mean_table(df: pd.DataFrame) -> Path:
    """Write a CSV the paper build can cite; also useful for LaTeX hand-copy."""
    rows = []
    for sc in SCENARIO_ORDER:
        for m in METHOD_ORDER:
            sub = df[(df["scenario"] == sc) & (df["method"] == m)]
            rows.append(
                {
                    "scenario": sc,
                    "method": m,
                    "collision_mean": sub["collision_per_1k_hrs"].mean(),
                    "collision_sd": sub["collision_per_1k_hrs"].std(ddof=0),
                    "fatal_mean": sub["fatal_per_1k_hrs"].mean(),
                    "fatal_sd": sub["fatal_per_1k_hrs"].std(ddof=0),
                    "prep_mean": sub["mean_p_prep"].mean(),
                    "prep_sd": sub["mean_p_prep"].std(ddof=0),
                    "iwrap_mean": sub["iwrap_nc_per_year"].mean(),
                }
            )
    out = ROOT / "outputs" / "definitive_means.csv"
    pd.DataFrame(rows).to_csv(out, index=False)
    print("Wrote", out)
    return out


def main() -> None:
    style()
    OUT.mkdir(parents=True, exist_ok=True)
    df = load_definitive()
    hyp = load_hypothesis()
    grid = load_storm_grid()
    write_mean_table(df)
    fig_kpi_distributions(df)
    fig_prep_collisions(df)
    fig_hypothesis_heatmap(hyp)
    fig_storm_sweep(grid)
    fig_route_density()
    print("Done.")


if __name__ == "__main__":
    main()
