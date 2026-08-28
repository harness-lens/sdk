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
