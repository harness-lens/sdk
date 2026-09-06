<!-- SPDX-License-Identifier: MPL-2.0 -->
<!-- Copyright © 2026 Cristian Camargo Filho -->

# Provider services

The Rust SDK implements Core's version 1 provider contracts at the local process
boundary. Catalog registration is compiled in and deterministic:

- `harness-lens-native` is MPL-2.0, built in, always available, and always
  selected for deterministic analysis and separate lexical evidence.
- `codeburn` is optional MIT-licensed external software. Version `0.9.24` is the
  only version covered by this adapter's current contract tests.

`ProviderService::catalog` accepts only known local IDs. Off mode and unselected
providers perform no detection. Snapshot mode checks configuration state without
starting CodeBurn. Live detection requires a trusted, non-virtual workspace and
runs `codeburn --version` without a shell, with a two-second deadline and a
4-KiB output bound. Results expose only normalized version or Core's fixed safe
error classes; stdout, stderr, arguments, paths, and credentials are not returned.

`ProviderService::installation_plan` returns a serialization-only preview for
the tested npm package version. `ProviderService::install` does not execute that
object. It reconstructs fixed npm arguments, requires explicit confirmation of
the exact version, and rechecks trusted/non-virtual policy immediately before a
bounded process launch. Native and unknown providers have no installation path.
No installation or provider execution occurs during SDK tests.

The SDK does not capture runtime reports or merge contributions. Language Server
owns bounded refresh orchestration and uses Core's `merge_reports` contract so
provider failures stay observable and cannot alter Native findings or scores.

## Verification

```bash
cd rust
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Tests inject a fake process boundary to prove off/snapshot non-execution, trust
blocking, unknown-provider rejection, safe status mapping, tested-version
enforcement, exact consent, and fixed installer reconstruction.
