// SPDX-License-Identifier: MPL-2.0
// Copyright © 2026 Cristian Camargo Filho

import { compareReports, scanRepository } from "@harness-lens/core";
import type {
  AiInterpreter,
  HarnessReport,
  ReportComparison,
  ScanOptions,
} from "@harness-lens/core";

export interface HarnessLensOptions {
  profile?: string;
  ai?: AiInterpreter;
}

export interface ScanResult {
  report: HarnessReport;
  interpretation: Awaited<ReturnType<AiInterpreter["interpret"]>> | null;
}

export interface HarnessLensDependencies {
  scan(repository: string, options?: ScanOptions): Promise<HarnessReport>;
}

export class HarnessLens {
  readonly #options: HarnessLensOptions;
  readonly #dependencies: HarnessLensDependencies;

  constructor(
    options: HarnessLensOptions = {},
    dependencies: HarnessLensDependencies = { scan: scanRepository },
  ) {
    this.#options = options;
    this.#dependencies = dependencies;
  }

  async scan(repository = process.cwd()): Promise<ScanResult> {
    const report = await this.#dependencies.scan(
      repository,
      this.#options.profile ? { profile: this.#options.profile } : {},
    );
    const interpretation = this.#options.ai ? await this.#options.ai.interpret(report) : null;
    return { report, interpretation };
  }

  compare(from: HarnessReport, to: HarnessReport): ReportComparison {
    return compareReports(from, to);
  }
}

export function createHarnessLens(options: HarnessLensOptions = {}): HarnessLens {
  return new HarnessLens(options);
}

export type {
  AiInterpretation,
  AiInterpreter,
  CoverageDetail,
  Finding,
  HarnessFileSummary,
  HarnessKind,
  HarnessReport,
  MetricEvaluation,
  Metrics,
  ReportComparison,
  ScanOptions,
  Severity,
} from "@harness-lens/core";
