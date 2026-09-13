// SPDX-License-Identifier: MPL-2.0
// Copyright © 2026 Cristian Camargo Filho

#![doc = include_str!("../README.md")]

mod filesystem;
pub mod observed_flow;
pub mod providers;
pub mod trace;

pub use filesystem::{
    DiscoveryResult, ScanError, Scanner, discover, discover_detailed, is_harness_path,
};
pub use harness_lens_config::{ConfigError, DEFAULT_CONFIG_FILE, load_for_root};
pub use harness_lens_core::lexical;
pub use harness_lens_core::{
    ACTION_TRACE_SCHEMA_VERSION, ActionIdentity, ActionObservation, ActionTrace, AnalysisEngine,
    AnalysisReport, CompletenessReason, ConfidenceEstimate, DiscoveryConfig, EvaluationConfig,
    EvidenceCompleteness, EvidenceLocation, Finding, FindingLocation, GraphAvailability, GraphEdge,
    GraphFilters, GraphKind, GraphLimits, GraphNode, GraphNodeKind, GraphProvenance,
    GraphRelationship, GraphValidationError, HarnessLensConfig, HarnessSource, HarnessSourceKind,
    IncompleteReason, IntegrationConfig, IntegrationError, Metric, ObservationWindow, ObservedCost,
    ObservedTokenUsage, Plugin, PluginConfig, PluginContext, PluginError, PluginExecution,
    PluginExecutionStatus, PluginMetadata, PluginOutput, RELATIONSHIP_GRAPH_SCHEMA_VERSION,
    RegistrationError, RelationshipGraph, ReportSink, RuntimeErrorClass, RuntimeMode,
    RuntimeObservationStatus, ScanCompleteness, ScanSummary, Score, ScoreCategory, ScoreError,
    ScoreMethod, ScoreSummary, Severity, SourceRecord, TextSpan, WeightedEdgeMetric, statistics,
};

/// Published Harness Lens namespace-bootstrap version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facade_exposes_core_summary() {
        assert!(ScanSummary::default().is_empty());
        assert_eq!(RuntimeMode::default(), RuntimeMode::Off);
    }
}
