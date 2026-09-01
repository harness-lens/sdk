// SPDX-License-Identifier: MPL-2.0
// Copyright © 2026 Cristian Camargo Filho

import assert from "node:assert/strict";
import test from "node:test";
import { HarnessLens, createHarnessLens } from "../dist/index.js";

const report = {
  schemaVersion: "harness-lens/report/v1",
  repository: "/repo",
  generatedAt: "2026-01-01T00:00:00.000Z",
  files: [],
  findings: [],
  metrics: {
    tokens: { count: 0, tokenizer: "heuristic/4-chars" },
    cost: { status: "not-evaluated", score: null, reference: null, details: null },
    coverage: { status: "not-evaluated", score: null, reference: null, details: null },
    alignment: { status: "not-evaluated", score: null, reference: null, details: null },
    redundancy: 0,
    conflicts: 0,
  },
};

test("scans through the stable facade", async () => {
  const sdk = new HarnessLens({ profile: "coding-agent/v1" }, {
    scan: async (_repository, options) => {
      assert.equal(options.profile, "coding-agent/v1");
      return report;
    },
  });
  const result = await sdk.scan("/repo");
  assert.equal(result.report, report);
  assert.equal(result.interpretation, null);
});

test("keeps AI interpretation separate from the report", async () => {
  const sdk = new HarnessLens({ ai: { interpret: async () => ({ simplifications: ["Shorten repeated rules"] }) } }, {
    scan: async () => report,
  });
  const result = await sdk.scan("/repo");
  assert.deepEqual(result.interpretation?.simplifications, ["Shorten repeated rules"]);
  assert.equal(result.report, report);
  assert.ok(createHarnessLens() instanceof HarnessLens);
});
