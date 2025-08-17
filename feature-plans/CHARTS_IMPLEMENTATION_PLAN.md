# Charts: Implementation Plan (MVP → Iterations)

This document outlines how we’ll add charts to Overseer using the DSL you confirmed:
- A generic `chart` node defines the drawing area and global options.
- Child `plot` nodes define individual series (multi-series charts).

## MVP (M1): Line plots from list data

Scope
- Backend computes per-plot series points from a source list using x/y expressions.
- Computed results are attached to plot nodes as internal parameters for the renderer.
- Frontend renders simple line charts on a canvas using the computed series and basic axes.

DSL (initial)
- chart ChartName (background-color=?, axis-color=?, grid-color=?, padding=?) {
    plot P1 (source="/Path/To/List", x=$(x/id), y=$(x/value), color=#4A90E2)
    plot P2 (source="../OtherList", x=$(x/when), y=$(x/score), color=red)
  }

Notes
- `source`: path string (absolute "/...", relative "A/B", or with "../").
- `x`, `y`: formulas or simple expressions (parsed like any formula body). `x` and `y` are evaluated per list item with `x` bound to the item.
- Computed points are stored as JSON in `_computed_series` on each `plot` node; chart-level bounds `_computed_x_min/_max/_y_min/_max` are floats.

Backend work
- Resolver adds a pass after formula evaluation to:
  1) Find `chart` nodes and their `plot` children.
  2) Resolve `source` to a node and iterate its accessible children as items.
  3) Evaluate `x`/`y` expressions per item via FormulaEvaluator (binding item as `x`).
  4) Coerce numbers; skip invalid points; compute min/max.
  5) Write `_computed_series` (JSON string) on plots and `_computed_*` bounds on the chart.

Frontend work
- Renderer reads `_computed_series` and draws lines on a canvas.
- Uses chart-level computed bounds for simple axes; respects `plot.color` when present.

## M2: Quality and options
- Domain overrides (domain-x-min/max, domain-y-min/max) on `chart`.
- Grid and tick drawing; axis labels; layout-driven canvas sizing.
- Line styles: width, dashed; optional points/area fill.

## M3: Multi-series and legends
- Legend block with series names/colors; toggling visibility.
- Multiple Y axes (optional, later).

## M4+: Advanced
- Non-time domains: categorical x mapping; bar charts.
- Interactions: hover tooltips, zoom/pan, selections.
- Synthetic domains: uniform spacing, function plots without a list source.

Testing
- Add resolver tests to validate `_computed_series` and bounds for a small synthetic list.
- Visual smoke test via example file once renderer draws lines.

---
Short, incremental, and renderer-friendly: backend produces stable computed data; frontend focuses on drawing.
