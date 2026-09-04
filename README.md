> SPDX-License-Identifier: MPL-2.0
> Copyright © 2026 Cristian Camargo Filho

# @harness-lens/sdk

Stable embedding facade for Harness Lens.

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

## Development

```bash
npm install
npm test
npm run check
```

## License

Early namespace-reservation versions used BSD-3-Clause. The official functional
implementation is licensed under MPL-2.0. When Covered Software is distributed,
modified MPL-covered files must remain available in Source Code Form under the
license. See [LICENSING](LICENSING.md), [COPYRIGHT](COPYRIGHT), and
[TRADEMARKS](TRADEMARKS).
