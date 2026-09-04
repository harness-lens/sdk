> SPDX-License-Identifier: MPL-2.0
> Copyright © 2026 Cristian Camargo Filho

# Publishing

## TypeScript

Publish `@harness-lens/core@0.0.1` first. Publish this package interactively once, then configure npm trusted publishing for `.github/workflows/publish.yml`.

```bash
npm login
npm publish --access public --provenance
```

Never commit npm tokens.

## Rust

Publish `harness-lens-config` and
`harness-lens-adapter-harness-score` before `harness-lens`. The manifests retain
version requirements so packaged crates resolve through crates.io; repository
builds additionally pin the core Git revision for reproducibility.

## Python

The `publish-to-pypi.yml` workflow builds stable-ABI wheels and an sdist from the
PyO3 workspace when a GitHub release is published. Configure the `pypi`
environment for trusted publishing before the first release. PyPI and npm share
the repository release tag but remain independently verifiable artifacts.
