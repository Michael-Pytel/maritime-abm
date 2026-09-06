#!/usr/bin/env python3
"""Backward-compatible entry point — delegates to plot_paper_figures.py."""
from __future__ import annotations

import runpy
from pathlib import Path

if __name__ == "__main__":
    runpy.run_path(str(Path(__file__).with_name("plot_paper_figures.py")), run_name="__main__")
