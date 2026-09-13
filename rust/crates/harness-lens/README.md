> SPDX-License-Identifier: MPL-2.0
> Copyright © 2026 Cristian Camargo Filho

# harness-lens

The native scanner includes `HL032` exact duplicate line/paragraph warnings,
both source locations, and normalization evidence. It also exposes source
budgets and caller-configured input-cost estimates through `EvaluationConfig`.

Reusable Rust SDK for Harness Lens. It combines the generic engine with a safe
filesystem adapter, repository-local configuration resolution, and first-party
deterministic plugins.

Use `discover()` for content-free path discovery or `Scanner::scan()` for a
complete evidence-bearing report. Add analyzers through `Scanner::register_plugin`.
`discover_detailed()` exposes incomplete-scan reasons, and
`Scanner::scan_with_overrides()` safely analyzes unsaved editor buffers.

Reports include bounded content-free inclusion edges for local inline Markdown
links. Each edge labels heuristic method and assumption; unresolved, ignored,
out-of-root, cyclic, and bounded states remain visible without loading or
serializing referenced source content.

`lexical` exposes Core's separate bounded lexical report. `providers` exposes
the fixed Native and CodeBurn catalog plus local detection and installation
boundaries. Optional processes stay off by default and cannot run for untrusted
or virtual workspaces.

`trace` parses a deny-by-default, content-safe action snapshot and normalizes
missing, redacted, duplicate, unordered, partial, and bounded records without
coercing unknown measurements to zero. Optional per-turn token usage remains
provider-neutral and records whether the source measured or estimated it.
`observed_flow` derives only original adjacent transitions. It applies
deterministic filters, recalculates declared denominators, layers repeated
logical actions to expose cycles, and emits the versioned Core
relationship-graph contract. `build_observed_flow_projection` also emits a
bounded, statistically labelled turn timeline aligned with visible graph
layers. Its sample size counts only turns carrying token evidence; absent token
evidence remains an explicit gap.

## License

Early namespace-reservation versions used BSD-3-Clause. The official functional
implementation is licensed under MPL-2.0. See [LICENSING](../../../LICENSING.md),
[COPYRIGHT](../../../COPYRIGHT), and [TRADEMARKS](../../../TRADEMARKS).
