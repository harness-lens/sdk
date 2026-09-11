<!-- SPDX-License-Identifier: MPL-2.0 -->
<!-- Copyright © 2026 Cristian Camargo Filho -->

# harness-lens-store

Bounded local persistence for completed, content-safe Harness Lens reports.

The first backend stores immutable JSON records in one directory. Keys cannot
escape that directory, reads and writes enforce byte limits, listings enforce
an entry limit, and existing records cannot be overwritten. The crate owns no
analysis rules, source collection, database dependency, network behavior, or
GUI lifecycle.

CLI, terminal, desktop, tests, and future hosts can select this backend or
implement the small `ReportStore` contract.

## Development

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

## License

MPL-2.0. See the owning repository license files.
