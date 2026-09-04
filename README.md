> SPDX-License-Identifier: MPL-2.0
> Copyright © 2026 Cristian Camargo Filho

# @harness-lens/sdk

Stable embedding facade for Harness Lens. This repository owns the SDK layer in
three forms while keeping one dependency direction toward the neutral core.

| Surface | Location | Purpose |
| --- | --- | --- |
| Rust | [`rust/`](rust/) | filesystem, config, plugin, and integration adapters |
| Python | [`src/harness_lens/`](src/harness_lens/) | PyO3-backed Python SDK |
| TypeScript | [`src/index.ts`](src/index.ts) | compatibility SDK for npm consumers |

```ts
import { createHarnessLens } from "@harness-lens/sdk";

const lens = createHarnessLens({ profile: "coding-agent/v1" });
const { report, interpretation } = await lens.scan(process.cwd());
```

The SDK delegates deterministic behavior to `@harness-lens/core`. Optional AI interpretation is returned beside the report and cannot mutate its findings or scores.

```ts
const delta = lens.compare(previousReport, currentReport);
```

Bootstrap order: publish `@harness-lens/core@0.0.1` before this package.

The Rust workspace pins the exact reference-core revision from
[`harness-lens/core`](https://github.com/harness-lens/core). It exposes a
transport-neutral Harness Score adapter seam; concrete network and authentication
behavior stays outside the analysis engine.

## Ecosystem

- [Core](https://github.com/harness-lens/core)
- [CLI](https://github.com/harness-lens/cli)
- [Language Server](https://github.com/harness-lens/language-server)
- [VS Code](https://github.com/harness-lens/harness-lens-vscode)
- [Project hub](https://github.com/harness-lens/harness-lens)

## Development

```bash
npm install
npm test
npm run check

cd rust
cargo test --workspace --locked

cd ..
python -m pip install -e ".[test]"
python -m pytest
```

## License

Early namespace-reservation versions used BSD-3-Clause. The official functional
implementation is licensed under MPL-2.0. When Covered Software is distributed,
modified MPL-covered files must remain available in Source Code Form under the
license. See [LICENSING](LICENSING.md), [COPYRIGHT](COPYRIGHT), and
[TRADEMARKS](TRADEMARKS).
