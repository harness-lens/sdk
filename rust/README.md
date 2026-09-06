<!-- SPDX-License-Identifier: MPL-2.0 -->
<!-- Copyright © 2026 Cristian Camargo Filho -->

# Harness Lens Rust SDK workspace

This workspace owns integration-facing Rust surfaces built on the provider-neutral
[`harness-lens-core`](https://github.com/harness-lens/core) engine:

- `harness-lens`: filesystem discovery and embeddable scanning facade
- `harness-lens-config`: TOML configuration adapter
- `harness-lens-adapter-harness-score`: transport-neutral report mapping seam
- `harness-lens-python`: PyO3 extension backing the Python SDK

The core dependency is pinned to an immutable Git revision until the crate is
published. CLI and editor transports remain in their own repositories.

Filesystem reports enrich Core's direct-discovery edges with bounded local
Markdown-reference edges. Reference extraction is heuristic and records its
assumption. Missing, cyclic, ignored, out-of-root, and edge-limit states remain
observable. Unsaved buffers overlay both analysis and reference extraction
without changing disk content.
