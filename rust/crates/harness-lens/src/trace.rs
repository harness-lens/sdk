// SPDX-License-Identifier: MPL-2.0
// Copyright © 2026 Cristian Camargo Filho

//! Normalization boundary for provider-neutral sanitized action traces.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::path::PathBuf;

use harness_lens_core::{
    ACTION_TRACE_SCHEMA_VERSION, ActionIdentity, ActionObservation, ActionTrace,
    CompletenessReason, EvidenceCompleteness, EvidenceDescriptor, EvidenceLocation,
    ObservationWindow, ObservedCost, ObservedTokenUsage, RuntimeErrorClass,
    RuntimeObservationStatus, TextSpan,
};
use serde::Deserialize;

/// Hard ceiling for programmatic normalization input.
pub const ABSOLUTE_MAX_TRACE_OBSERVATIONS: usize = 100_000;

/// Safe source-declared limitation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceSourceIssue {
    /// Source omitted observations.
    Dropped,
    /// Source redacted one or more values.
    Redacted,
    /// Source ended before its declared window completed.
    Interrupted,
    /// Source could not decode some records.
    Incompatible,
    /// Source applied its own bound.
    Truncated,
}

impl TraceSourceIssue {
    fn code(self) -> &'static str {
        match self {
            Self::Dropped => "source_dropped",
            Self::Redacted => "source_redacted",
            Self::Interrupted => "source_interrupted",
            Self::Incompatible => "source_incompatible",
            Self::Truncated => "source_truncated",
        }
    }
}

/// Field explicitly removed before Harness Lens receives an observation.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedactedTraceField {
    /// Observation identity.
    Id,
    /// Session identity.
    SessionId,
    /// Session order.
    Sequence,
    /// Stable action identity.
    ActionId,
    /// Display label.
    ActionLabel,
    /// Action category.
    Category,
    /// Terminal status.
    Status,
    /// Observation timestamp.
    ObservedAt,
    /// Duration.
    DurationMicros,
    /// Retry count.
    RetryCount,
    /// Attributed cost.
    Cost,
    /// Per-turn token usage.
    TokenUsage,
    /// Model identity.
    Model,
    /// Harness asset identity.
    AssetIdentity,
    /// Immutable revision.
    Revision,
    /// Source location.
    Location,
}

impl RedactedTraceField {
    fn required(self) -> bool {
        matches!(
            self,
            Self::Id
                | Self::SessionId
                | Self::Sequence
                | Self::ActionId
                | Self::ActionLabel
                | Self::Category
                | Self::Status
        )
    }
}

/// Content-safe action identity accepted at the SDK boundary.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceActionInput {
    /// Stable logical identity.
    pub id: Option<String>,
    /// Safe display label.
    pub label: Option<String>,
    /// Stable filter category.
    pub category: Option<String>,
}

/// Content-safe location accepted at the SDK boundary.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceLocationInput {
    /// Workspace-relative path.
    pub path: Option<PathBuf>,
    /// One-based line.
    pub line: Option<usize>,
    /// UTF-8 byte span.
    pub span: Option<TextSpan>,
}

/// Content-safe cost accepted at the SDK boundary.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceCostInput {
    /// Non-negative amount.
    pub value: f64,
    /// Currency or billing unit.
    pub unit: String,
    /// Whether pricing inputs were estimated.
    pub estimated: bool,
}

/// Content-safe token usage accepted at the SDK boundary.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceTokenUsageInput {
    /// Prompt or input tokens, when supplied.
    pub input_tokens: Option<u64>,
    /// Completion or output tokens, when supplied.
    pub output_tokens: Option<u64>,
    /// Cached input tokens, when supplied.
    pub cached_input_tokens: Option<u64>,
    /// Total tokens attributed to this turn.
    pub total_tokens: u64,
    /// Whether any count was estimated by the source.
    pub estimated: bool,
}

/// One possibly incomplete source observation.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceObservationInput {
    /// Stable observation identity.
    pub id: Option<String>,
    /// Stable session identity.
    pub session_id: Option<String>,
    /// Provider-supplied order within the session.
    pub sequence: Option<u64>,
    /// Normalized timestamp, when available.
    pub observed_at: Option<String>,
    /// Safe action identity.
    pub action: Option<TraceActionInput>,
    /// Sanitized terminal status.
    pub status: Option<RuntimeObservationStatus>,
    /// Duration; absence remains unknown.
    pub duration_micros: Option<u64>,
    /// Retry count; absence remains unknown.
    pub retry_count: Option<u32>,
    /// Attributed cost, when available.
    pub cost: Option<TraceCostInput>,
    /// Per-turn token usage, when available.
    pub token_usage: Option<TraceTokenUsageInput>,
    /// Stable error class, when available.
    pub error_class: Option<RuntimeErrorClass>,
    /// Safe model identity, when available.
    pub model: Option<String>,
    /// Safe harness asset identity, when available.
    pub asset_identity: Option<String>,
    /// Immutable revision, when available.
    pub revision: Option<String>,
    /// Safe source navigation location, when available.
    pub location: Option<TraceLocationInput>,
    /// Explicitly redacted fields.
    #[serde(default)]
    pub redacted_fields: Vec<RedactedTraceField>,
}

/// Versioned local trace input accepted from a supported adapter or snapshot.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SanitizedTraceInput {
    /// Input schema version.
    pub schema_version: u32,
    /// Inclusive observation window.
    pub window: Option<ObservationWindow>,
    /// Source completeness declaration.
    pub complete: Option<bool>,
    /// Safe source limitations.
    #[serde(default)]
    pub source_issues: Vec<TraceSourceIssue>,
    /// Possibly unordered observations.
    #[serde(default)]
    pub observations: Vec<TraceObservationInput>,
    /// Total source observations before source-side bounds.
    pub total_observations: Option<usize>,
    /// Safe continuation token, when supplied.
    pub next_cursor: Option<String>,
}

/// Stable normalization issue class containing no source content.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum TraceNormalizationIssueCode {
    /// Source omitted its completeness declaration.
    MissingCompleteness,
    /// Required field was absent.
    MissingRequiredField,
    /// Required field was redacted, so the record could not be ordered safely.
    RedactedRequiredField,
    /// Optional field was redacted and remains unknown.
    RedactedOptionalField,
    /// Observation failed Core content-safe validation.
    InvalidObservation,
    /// Repeated observation identity was dropped.
    DuplicateObservation,
    /// Ambiguous session sequence position was dropped.
    DuplicateSequence,
    /// Source order differed from canonical session order.
    ReorderedObservations,
    /// SDK output bound omitted otherwise valid observations.
    TruncatedObservations,
}

impl TraceNormalizationIssueCode {
    fn code(self) -> &'static str {
        match self {
            Self::MissingCompleteness => "missing_completeness",
            Self::MissingRequiredField => "missing_required_field",
            Self::RedactedRequiredField => "redacted_required_field",
            Self::RedactedOptionalField => "redacted_optional_field",
            Self::InvalidObservation => "invalid_observation",
            Self::DuplicateObservation => "duplicate_observation",
            Self::DuplicateSequence => "duplicate_sequence",
            Self::ReorderedObservations => "reordered_observations",
            Self::TruncatedObservations => "truncated_observations",
        }
    }
}

/// One content-free normalization issue.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceNormalizationIssue {
    /// Stable issue class.
    pub code: TraceNormalizationIssueCode,
    /// Input record index, when one record caused the issue.
    pub input_index: Option<usize>,
}

/// Canonical trace plus observable normalization output.
#[derive(Clone, Debug, PartialEq)]
pub struct NormalizedTrace {
    /// Canonical Core trace.
    pub trace: ActionTrace,
    /// Stable content-free issues.
    pub issues: Vec<TraceNormalizationIssue>,
}

/// Failure before a canonical trace can be produced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraceInputError {
    /// JSON did not match the deny-by-default safe schema.
    InvalidJson,
    /// Schema version is unsupported.
    UnsupportedSchemaVersion(u32),
    /// Observation window is missing or invalid.
    InvalidWindow,
    /// Requested output limit is zero or above the hard ceiling.
    InvalidLimit,
    /// Declared total is smaller than supplied observations.
    InvalidTotal,
    /// Canonical Core validation failed.
    InvalidContract,
}

impl fmt::Display for TraceInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson => formatter.write_str("invalid sanitized trace JSON"),
            Self::UnsupportedSchemaVersion(version) => {
                write!(
                    formatter,
                    "unsupported sanitized trace schema version: {version}"
                )
            }
            Self::InvalidWindow => formatter.write_str("sanitized trace requires a valid window"),
            Self::InvalidLimit => formatter.write_str("invalid trace observation limit"),
            Self::InvalidTotal => {
                formatter.write_str("trace total is smaller than supplied observations")
            }
            Self::InvalidContract => formatter.write_str("normalized trace violates Core contract"),
        }
    }
}

impl Error for TraceInputError {}

/// Parses deny-by-default JSON and normalizes it into the Core trace contract.
pub fn normalize_trace_json(
    value: &str,
    max_observations: usize,
) -> Result<NormalizedTrace, TraceInputError> {
    let input = serde_json::from_str(value).map_err(|_| TraceInputError::InvalidJson)?;
    normalize_trace(input, max_observations)
}

/// Normalizes missing, redacted, duplicate, unordered, and bounded input explicitly.
pub fn normalize_trace(
    input: SanitizedTraceInput,
    max_observations: usize,
) -> Result<NormalizedTrace, TraceInputError> {
    if input.schema_version != ACTION_TRACE_SCHEMA_VERSION {
        return Err(TraceInputError::UnsupportedSchemaVersion(
            input.schema_version,
        ));
    }
    if max_observations == 0
        || max_observations > ABSOLUTE_MAX_TRACE_OBSERVATIONS
        || input.observations.len() > ABSOLUTE_MAX_TRACE_OBSERVATIONS
    {
        return Err(TraceInputError::InvalidLimit);
    }
    let window = input.window.ok_or(TraceInputError::InvalidWindow)?;
    if input
        .total_observations
        .is_some_and(|total| total < input.observations.len())
    {
        return Err(TraceInputError::InvalidTotal);
    }

    let mut issues = Vec::new();
    let mut reasons = BTreeMap::<String, usize>::new();
    if input.complete.is_none() {
        record_issue(
            &mut issues,
            &mut reasons,
            TraceNormalizationIssueCode::MissingCompleteness,
            None,
        );
    }
    for issue in &input.source_issues {
        *reasons.entry(issue.code().to_owned()).or_default() += 1;
    }
    if input.complete == Some(false) && input.source_issues.is_empty() {
        *reasons.entry("source_incomplete".to_owned()).or_default() += 1;
    }
    if input.next_cursor.is_some()
        || input
            .total_observations
            .is_some_and(|total| total > input.observations.len())
    {
        *reasons.entry("source_truncated".to_owned()).or_default() += 1;
    }

    let mut candidates = Vec::new();
    for (index, observation) in input.observations.into_iter().enumerate() {
        let redacted = observation
            .redacted_fields
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        if redacted.iter().any(|field| field.required()) {
            record_issue(
                &mut issues,
                &mut reasons,
                TraceNormalizationIssueCode::RedactedRequiredField,
                Some(index),
            );
            continue;
        }
        if redacted.iter().any(|field| !field.required()) {
            record_issue(
                &mut issues,
                &mut reasons,
                TraceNormalizationIssueCode::RedactedOptionalField,
                Some(index),
            );
        }
        let Some(id) = observation.id else {
            record_issue(
                &mut issues,
                &mut reasons,
                TraceNormalizationIssueCode::MissingRequiredField,
                Some(index),
            );
            continue;
        };
        let Some(session_id) = observation.session_id else {
            record_issue(
                &mut issues,
                &mut reasons,
                TraceNormalizationIssueCode::MissingRequiredField,
                Some(index),
            );
            continue;
        };
        let Some(sequence) = observation.sequence else {
            record_issue(
                &mut issues,
                &mut reasons,
                TraceNormalizationIssueCode::MissingRequiredField,
                Some(index),
            );
            continue;
        };
        let Some(action) = observation.action else {
            record_issue(
                &mut issues,
                &mut reasons,
                TraceNormalizationIssueCode::MissingRequiredField,
                Some(index),
            );
            continue;
        };
        let (Some(action_id), Some(label), Some(category), Some(status)) =
            (action.id, action.label, action.category, observation.status)
        else {
            record_issue(
                &mut issues,
                &mut reasons,
                TraceNormalizationIssueCode::MissingRequiredField,
                Some(index),
            );
            continue;
        };

        let location = if redacted.contains(&RedactedTraceField::Location) {
            None
        } else {
            observation.location.and_then(|location| {
                location.path.map(|path| EvidenceLocation {
                    path,
                    line: location.line,
                    span: location.span,
                })
            })
        };
        let cost = if redacted.contains(&RedactedTraceField::Cost) {
            None
        } else {
            observation.cost.map(|cost| ObservedCost {
                value: cost.value,
                unit: cost.unit,
                estimated: cost.estimated,
            })
        };
        let token_usage = if redacted.contains(&RedactedTraceField::TokenUsage) {
            None
        } else {
            observation.token_usage.map(|usage| ObservedTokenUsage {
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                cached_input_tokens: usage.cached_input_tokens,
                total_tokens: usage.total_tokens,
                estimated: usage.estimated,
            })
        };
        let value = ActionObservation {
            id,
            session_id,
            sequence,
            observed_at: (!redacted.contains(&RedactedTraceField::ObservedAt))
                .then_some(observation.observed_at)
                .flatten(),
            action: ActionIdentity {
                id: action_id,
                label,
                category,
            },
            status,
            duration_micros: (!redacted.contains(&RedactedTraceField::DurationMicros))
                .then_some(observation.duration_micros)
                .flatten(),
            retry_count: (!redacted.contains(&RedactedTraceField::RetryCount))
                .then_some(observation.retry_count)
                .flatten(),
            cost,
            token_usage,
            error_class: observation.error_class,
            model: (!redacted.contains(&RedactedTraceField::Model))
                .then_some(observation.model)
                .flatten(),
            asset_identity: (!redacted.contains(&RedactedTraceField::AssetIdentity))
                .then_some(observation.asset_identity)
                .flatten(),
            revision: (!redacted.contains(&RedactedTraceField::Revision))
                .then_some(observation.revision)
                .flatten(),
            location,
            evidence: EvidenceDescriptor::default(),
        };
        if value.validate().is_err() {
            record_issue(
                &mut issues,
                &mut reasons,
                TraceNormalizationIssueCode::InvalidObservation,
                Some(index),
            );
            continue;
        }
        candidates.push((index, value));
    }

    if !candidates.windows(2).all(|pair| {
        let left = &pair[0].1;
        let right = &pair[1].1;
        (&left.session_id, left.sequence, &left.id)
            <= (&right.session_id, right.sequence, &right.id)
    }) {
        record_issue(
            &mut issues,
            &mut reasons,
            TraceNormalizationIssueCode::ReorderedObservations,
            None,
        );
    }
    candidates.sort_by(|left, right| {
        let left = &left.1;
        let right = &right.1;
        (&left.session_id, left.sequence, &left.id).cmp(&(
            &right.session_id,
            right.sequence,
            &right.id,
        ))
    });

    let mut id_counts = BTreeMap::<String, usize>::new();
    let mut position_counts = BTreeMap::<(String, u64), usize>::new();
    for (_, observation) in &candidates {
        *id_counts.entry(observation.id.clone()).or_default() += 1;
        *position_counts
            .entry((observation.session_id.clone(), observation.sequence))
            .or_default() += 1;
    }
    let mut observations = Vec::new();
    for (index, observation) in candidates {
        if id_counts[&observation.id] > 1 {
            record_issue(
                &mut issues,
                &mut reasons,
                TraceNormalizationIssueCode::DuplicateObservation,
                Some(index),
            );
            continue;
        }
        if position_counts[&(observation.session_id.clone(), observation.sequence)] > 1 {
            record_issue(
                &mut issues,
                &mut reasons,
                TraceNormalizationIssueCode::DuplicateSequence,
                Some(index),
            );
            continue;
        }
        observations.push(observation);
    }
    if observations.len() > max_observations {
        let omitted = observations.len() - max_observations;
        observations.truncate(max_observations);
        *reasons
            .entry(
                TraceNormalizationIssueCode::TruncatedObservations
                    .code()
                    .to_owned(),
            )
            .or_default() += omitted;
        issues.push(TraceNormalizationIssue {
            code: TraceNormalizationIssueCode::TruncatedObservations,
            input_index: None,
        });
    }

    let completeness_reasons = reasons
        .into_iter()
        .map(|(code, count)| CompletenessReason {
            code,
            count: Some(count),
        })
        .collect::<Vec<_>>();
    let trace = ActionTrace {
        schema_version: ACTION_TRACE_SCHEMA_VERSION,
        window,
        completeness: EvidenceCompleteness {
            complete: input.complete == Some(true) && completeness_reasons.is_empty(),
            reasons: completeness_reasons,
        },
        observations,
        total_observations: input.total_observations,
        next_cursor: input.next_cursor,
    }
    .canonicalize();
    trace
        .validate(max_observations)
        .map_err(|_| TraceInputError::InvalidContract)?;
    issues.sort_by_key(|issue| (issue.input_index, issue.code));
    Ok(NormalizedTrace { trace, issues })
}

fn record_issue(
    issues: &mut Vec<TraceNormalizationIssue>,
    reasons: &mut BTreeMap<String, usize>,
    code: TraceNormalizationIssueCode,
    input_index: Option<usize>,
) {
    *reasons.entry(code.code().to_owned()).or_default() += 1;
    issues.push(TraceNormalizationIssue { code, input_index });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> SanitizedTraceInput {
        SanitizedTraceInput {
            schema_version: 1,
            window: Some(ObservationWindow {
                start: "2026-09-13T00:00:00Z".to_owned(),
                end: "2026-09-13T01:00:00Z".to_owned(),
            }),
            complete: Some(true),
            source_issues: Vec::new(),
            observations: vec![observation("second", 2), observation("first", 1)],
            total_observations: Some(2),
            next_cursor: None,
        }
    }

    fn observation(id: &str, sequence: u64) -> TraceObservationInput {
        TraceObservationInput {
            id: Some(id.to_owned()),
            session_id: Some("session-1".to_owned()),
            sequence: Some(sequence),
            observed_at: None,
            action: Some(TraceActionInput {
                id: Some(if sequence == 1 { "read" } else { "write" }.to_owned()),
                label: Some(if sequence == 1 { "Read" } else { "Write" }.to_owned()),
                category: Some("tool".to_owned()),
            }),
            status: Some(RuntimeObservationStatus::Success),
            duration_micros: None,
            retry_count: None,
            cost: None,
            token_usage: Some(TraceTokenUsageInput {
                input_tokens: Some(80),
                output_tokens: Some(40),
                cached_input_tokens: Some(20),
                total_tokens: 120,
                estimated: false,
            }),
            error_class: None,
            model: None,
            asset_identity: None,
            revision: None,
            location: None,
            redacted_fields: Vec::new(),
        }
    }

    #[test]
    fn reorders_without_fabricating_missing_measurements() {
        let result = normalize_trace(input(), 10).unwrap();
        assert_eq!(result.trace.observations[0].id, "first");
        assert_eq!(result.trace.observations[0].duration_micros, None);
        assert_eq!(result.trace.observations[0].retry_count, None);
        assert_eq!(
            result.trace.observations[0]
                .token_usage
                .as_ref()
                .map(|usage| usage.total_tokens),
            Some(120)
        );
        assert_eq!(
            result.issues[0].code,
            TraceNormalizationIssueCode::ReorderedObservations
        );
        assert!(!result.trace.completeness.complete);
    }

    #[test]
    fn drops_ambiguous_and_redacted_required_records_explicitly() {
        let mut value = input();
        value.observations = vec![
            observation("duplicate", 1),
            observation("duplicate", 2),
            TraceObservationInput {
                redacted_fields: vec![RedactedTraceField::SessionId],
                ..observation("redacted", 3)
            },
        ];
        value.total_observations = Some(3);

        let result = normalize_trace(value, 10).unwrap();
        assert!(result.trace.observations.is_empty());
        assert!(
            result
                .trace
                .completeness
                .reasons
                .iter()
                .any(|reason| reason.code == "duplicate_observation")
        );
        assert!(
            result
                .trace
                .completeness
                .reasons
                .iter()
                .any(|reason| reason.code == "redacted_required_field")
        );
    }

    #[test]
    fn json_boundary_rejects_payload_fields() {
        let value = r#"{
          "schema_version": 1,
          "window": {"start":"a","end":"z"},
          "complete": true,
          "observations": [{
            "id":"one","session_id":"s","sequence":1,
            "action":{"id":"read","label":"Read","category":"tool"},
            "status":"success","arguments":{"path":"secret"}
          }]
        }"#;
        assert_eq!(
            normalize_trace_json(value, 10),
            Err(TraceInputError::InvalidJson)
        );
    }

    #[test]
    fn invalid_token_usage_is_dropped_without_fabricating_zero() {
        let mut value = input();
        value.observations[0].token_usage = Some(TraceTokenUsageInput {
            input_tokens: Some(100),
            output_tokens: Some(50),
            cached_input_tokens: Some(20),
            total_tokens: 149,
            estimated: false,
        });

        let result = normalize_trace(value, 10).unwrap();
        assert_eq!(result.trace.observations.len(), 1);
        assert_eq!(result.trace.observations[0].id, "first");
        assert!(
            result
                .trace
                .completeness
                .reasons
                .iter()
                .any(|reason| { reason.code == "invalid_observation" && reason.count == Some(1) })
        );
    }

    #[test]
    fn fixtures_cover_missing_duplicate_redacted_partial_and_unordered_input() {
        let cases = [
            (
                include_str!("../test-data/traces/out-of-order.json"),
                "reordered_observations",
            ),
            (
                include_str!("../test-data/traces/missing.json"),
                "missing_required_field",
            ),
            (
                include_str!("../test-data/traces/duplicate.json"),
                "duplicate_observation",
            ),
            (
                include_str!("../test-data/traces/redacted.json"),
                "redacted_required_field",
            ),
            (
                include_str!("../test-data/traces/partial.json"),
                "source_dropped",
            ),
        ];
        for (value, expected) in cases {
            let result = normalize_trace_json(value, 10).unwrap();
            assert!(
                result
                    .trace
                    .completeness
                    .reasons
                    .iter()
                    .any(|reason| reason.code == expected),
                "missing {expected}"
            );
        }
    }
}
