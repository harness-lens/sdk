> SPDX-License-Identifier: MPL-2.0
> Copyright © 2026 Cristian Camargo Filho

# How to contribute

Read the central [ecosystem contribution flow](https://github.com/harness-lens/harness-lens/blob/main/docs/architecture.md#how-to-contribute),
[architecture rules](https://github.com/harness-lens/harness-lens/blob/main/docs/architecture.md#architecture-rules),
and [CI/test map](https://github.com/harness-lens/harness-lens/blob/main/docs/architecture.md#ci-and-test-map).
SDK owns safe discovery, configuration, embedding, Python/PyO3, and provider
services. Domain findings and scores belong in Core. See
[provider services](https://github.com/harness-lens/sdk/blob/main/docs/provider-services.md)
and the [Rust workspace guide](https://github.com/harness-lens/sdk/blob/main/rust/README.md).

Keep this facade small and backward-compatible. Integrations depend on the SDK
contract. Run:

```bash
npm ci
npm test
npm run check

cd rust
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

cd ..
python -m pip install -e ".[test]"
python -m compileall -q src tests
python -m pytest
```

## Licensing contributions

Contributions intentionally submitted to this repository are provided under
MPL-2.0. You must have the necessary rights to submit the work. When Covered
Software is distributed, modifications to MPL-covered files remain subject to
the Source Code Form obligations in the license.
