// SPDX-License-Identifier: MPL-2.0
// Copyright © 2026 Cristian Camargo Filho

//! Deterministic observed-transition aggregation.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::error::Error;
use std::fmt;

use harness_lens_core::{
    ActionIdentity, ActionObservation, ActionTrace, CompletenessReason, EvidenceCompleteness,
    EvidenceLocation, GraphAvailability, GraphEdge, GraphFilters, GraphKind, GraphLimits,
    GraphNode, GraphNodeKind, GraphProvenance, GraphRelationship, ObservationWindow, ObservedCost,
    ObservedTokenUsage, RELATIONSHIP_GRAPH_SCHEMA_VERSION, RelationshipGraph,
    RuntimeObservationStatus, ScoreMethod, WeightedEdgeMetric,
};
use serde::Serialize;

/// Hard ceiling for turns serialized beside one observed-flow graph.
pub const ABSOLUTE_MAX_FLOW_TURNS: usize = 10_000;

/// Supported single-unit width calculations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FlowMetric {
    /// Count ordered adjacent transitions.
    Transitions,
    /// Count distinct sessions containing each transition.
    DistinctSessions,
    /// Sum destination-action duration in microseconds.
    DurationMicros,
    /// Sum destination-action cost in one declared currency or billing unit.
    Cost(String),
}

impl FlowMetric {
    /// Returns the unit serialized on every weighted edge.
    #[must_use]
    pub fn unit(&self) -> &str {
        match self {
            Self::Transitions => "transitions",
            Self::DistinctSessions => "sessions",
            Self::DurationMicros => "microseconds",
            Self::Cost(unit) => unit,
        }
    }
}

/// Deterministic filters and response limits for observed flow.
#[derive(Clone, Debug, PartialEq)]
pub struct ObservedFlowOptions {
    /// Optional canonical action identity used as directed traversal root.
    pub root: Option<String>,
    /// Maximum serialized nodes.
    pub max_nodes: usize,
    /// Maximum serialized edges.
    pub max_edges: usize,
    /// Maximum directed hops from a selected root.
    pub max_hops: usize,
    /// Optional inclusive timestamp window.
    pub window: Option<ObservationWindow>,
    /// Destination-action categories to retain; empty selects all.
    pub categories: BTreeSet<String>,
    /// Destination-action statuses to retain; empty selects all.
    pub statuses: BTreeSet<RuntimeObservationStatus>,
    /// Minimum share after semantic filters and before response bounds.
    pub minimum_share: Option<f64>,
    /// Exactly one width metric.
    pub metric: FlowMetric,
}

impl Default for ObservedFlowOptions {
    fn default() -> Self {
        Self {
            root: None,
            max_nodes: 256,
            max_edges: 512,
            max_hops: 32,
            window: None,
            categories: BTreeSet::new(),
            statuses: BTreeSet::new(),
            minimum_share: None,
            metric: FlowMetric::Transitions,
        }
    }
}

/// One content-safe ordered turn available to a timeline renderer.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ObservedFlowTurn {
    /// Stable observation identity.
    pub id: String,
    /// Stable session identity.
    pub session_id: String,
    /// Provider-supplied order within the session.
    pub sequence: u64,
    /// Zero-based layer matching the Sankey projection.
    pub layer: usize,
    /// Normalized timestamp, when supplied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_at: Option<String>,
    /// Content-safe action identity.
    pub action: ActionIdentity,
    /// Sanitized terminal status.
    pub status: RuntimeObservationStatus,
    /// Measured or explicitly estimated token usage, when supplied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_usage: Option<ObservedTokenUsage>,
    /// Attributed cost, when supplied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<ObservedCost>,
    /// Safe source navigation location, when supplied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<EvidenceLocation>,
}

/// Bounded token evidence aligned with one observed-flow projection.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ObservedTokenTimeline {
    /// Deterministic projection over statistically sampled runtime evidence.
    pub method: ScoreMethod,
    /// Whether any visible turn carries token evidence.
    pub availability: GraphAvailability,
    /// Missing and truncated evidence reasons.
    pub completeness: EvidenceCompleteness,
    /// Maximum turns serialized in this response.
    pub max_turns: usize,
    /// Matching turns before the response bound.
    pub total_turns: usize,
    /// Visible turns carrying token evidence.
    pub sample_size: usize,
    /// Fixed unit for every token bar.
    pub unit: String,
    /// Canonically ordered visible turns, including explicit token gaps.
    pub turns: Vec<ObservedFlowTurn>,
}

/// Observed graph and its aligned bounded per-turn token projection.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ObservedFlowProjection {
    /// Aggregated transition graph.
    pub graph: RelationshipGraph,
    /// Ordered turn-level token evidence.
    pub token_timeline: ObservedTokenTimeline,
}

/// Failure to build a valid bounded observed-flow graph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FlowBuildError {
    /// Trace violates the Core contract.
    InvalidTrace,
    /// Filter or bound is invalid.
    InvalidOptions,
    /// Stable compact identity collided.
    IdentityCollision,
    /// Produced graph violates the Core contract.
    InvalidGraph,
}

impl fmt::Display for FlowBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidTrace => "invalid action trace",
            Self::InvalidOptions => "invalid observed-flow options",
            Self::IdentityCollision => "observed-flow identity collision",
            Self::InvalidGraph => "invalid observed-flow graph",
        })
    }
}

impl Error for FlowBuildError {}

#[derive(Clone)]
struct TransitionSample<'a> {
    source: &'a ActionObservation,
    target: &'a ActionObservation,
    source_layer: usize,
    target_layer: usize,
    value: f64,
}

#[derive(Clone)]
struct TransitionAggregate<'a> {
    source: &'a ActionObservation,
    target: &'a ActionObservation,
    source_layer: usize,
    target_layer: usize,
    value: f64,
    samples: usize,
    sessions: BTreeSet<&'a str>,
    evidence_ids: BTreeSet<&'a str>,
}

impl<'a> TransitionAggregate<'a> {
    fn add(&mut self, sample: &TransitionSample<'a>) {
        self.value += sample.value;
        self.samples += 1;
        self.sessions.insert(&sample.target.session_id);
        self.evidence_ids.insert(&sample.target.id);
        if (
            sample.target.session_id.as_str(),
            sample.target.sequence,
            sample.target.id.as_str(),
        ) < (
            self.target.session_id.as_str(),
            self.target.sequence,
            self.target.id.as_str(),
        ) {
            self.source = sample.source;
            self.target = sample.target;
        }
    }
}

/// Derives only original adjacent observed transitions, then applies filters and bounds.
pub fn build_observed_flow(
    trace: &ActionTrace,
    options: &ObservedFlowOptions,
) -> Result<RelationshipGraph, FlowBuildError> {
    trace
        .validate(trace.observations.len().max(1))
        .map_err(|_| FlowBuildError::InvalidTrace)?;
    validate_options(options)?;

    let mut reasons = trace
        .completeness
        .reasons
        .iter()
        .map(|reason| (reason.code.clone(), reason.count.unwrap_or(1)))
        .collect::<BTreeMap<_, _>>();
    let mut grouped = BTreeMap::<&str, Vec<&ActionObservation>>::new();
    for observation in &trace.observations {
        grouped
            .entry(&observation.session_id)
            .or_default()
            .push(observation);
    }

    let mut samples = Vec::new();
    for observations in grouped.values() {
        for (layer, pair) in observations.windows(2).enumerate() {
            let source = pair[0];
            let target = pair[1];
            if !matches_filters(source, target, options, &mut reasons) {
                continue;
            }
            let Some(value) = metric_value(target, &options.metric, &mut reasons) else {
                continue;
            };
            samples.push(TransitionSample {
                source,
                target,
                source_layer: layer,
                target_layer: layer + 1,
                value,
            });
        }
    }

    let mut aggregates = BTreeMap::<(usize, &str, usize, &str), TransitionAggregate<'_>>::new();
    for sample in &samples {
        let key = (
            sample.source_layer,
            sample.source.action.id.as_str(),
            sample.target_layer,
            sample.target.action.id.as_str(),
        );
        aggregates
            .entry(key)
            .and_modify(|aggregate| aggregate.add(sample))
            .or_insert_with(|| TransitionAggregate {
                source: sample.source,
                target: sample.target,
                source_layer: sample.source_layer,
                target_layer: sample.target_layer,
                value: sample.value,
                samples: 1,
                sessions: BTreeSet::from([sample.target.session_id.as_str()]),
                evidence_ids: BTreeSet::from([sample.target.id.as_str()]),
            });
    }
    if options.metric == FlowMetric::DistinctSessions {
        for aggregate in aggregates.values_mut() {
            aggregate.value = aggregate.sessions.len() as f64;
        }
    }

    let mut aggregates = aggregates.into_values().collect::<Vec<_>>();
    if let Some(root) = &options.root {
        aggregates = retain_reachable(aggregates, root, options.max_hops, &mut reasons);
    }
    let semantic_denominator = aggregates.iter().map(|edge| edge.value).sum::<f64>();
    if let Some(minimum) = options.minimum_share {
        if semantic_denominator > 0.0 {
            aggregates.retain(|edge| edge.value / semantic_denominator >= minimum);
        }
    }
    let denominator = aggregates.iter().map(|edge| edge.value).sum::<f64>();

    let before_bounds = aggregates.len();
    aggregates.sort_by(|left, right| {
        right
            .value
            .partial_cmp(&left.value)
            .unwrap_or(Ordering::Equal)
            .then_with(|| aggregate_key(left).cmp(&aggregate_key(right)))
    });
    let mut selected = Vec::new();
    let mut selected_nodes = BTreeSet::new();
    for aggregate in aggregates {
        let source = node_key(aggregate.source_layer, &aggregate.source.action);
        let target = node_key(aggregate.target_layer, &aggregate.target.action);
        let additional = usize::from(!selected_nodes.contains(&source))
            + usize::from(!selected_nodes.contains(&target));
        if selected.len() >= options.max_edges
            || selected_nodes.len() + additional > options.max_nodes
        {
            continue;
        }
        selected_nodes.insert(source);
        selected_nodes.insert(target);
        selected.push(aggregate);
    }
    if selected.len() < before_bounds {
        let code = if selected.len() >= options.max_edges {
            "truncated_edges"
        } else {
            "truncated_nodes"
        };
        *reasons.entry(code.to_owned()).or_default() += before_bounds - selected.len();
    }

    let availability = if !selected.is_empty() && denominator > 0.0 {
        GraphAvailability::Ready
    } else if trace.observations.is_empty() {
        if trace.completeness.complete {
            GraphAvailability::Empty
        } else {
            GraphAvailability::Unavailable
        }
    } else if trace.observations.len() < 2 || (!samples.is_empty() && denominator == 0.0) {
        GraphAvailability::InsufficientEvidence
    } else {
        GraphAvailability::Empty
    };

    let mut node_records = BTreeMap::<String, GraphNode>::new();
    let mut edge_records = Vec::new();
    let mut compact_ids = BTreeMap::<String, String>::new();
    if availability == GraphAvailability::Ready {
        for aggregate in selected {
            let source_key = node_key(aggregate.source_layer, &aggregate.source.action);
            let target_key = node_key(aggregate.target_layer, &aggregate.target.action);
            let source = graph_node(aggregate.source_layer, aggregate.source);
            let target = graph_node(aggregate.target_layer, aggregate.target);
            insert_compact_identity(&mut compact_ids, &source.id, &source_key)?;
            insert_compact_identity(&mut compact_ids, &target.id, &target_key)?;
            node_records.entry(source.id.clone()).or_insert(source);
            node_records.entry(target.id.clone()).or_insert(target);
            let edge_key = format!("{source_key}:{target_key}");
            let edge_id = compact_id("edge", &edge_key);
            insert_compact_identity(&mut compact_ids, &edge_id, &edge_key)?;
            let evidence_ids = aggregate
                .evidence_ids
                .iter()
                .take(32)
                .map(|value| (*value).to_owned())
                .collect::<Vec<_>>();
            edge_records.push(GraphEdge {
                id: edge_id,
                source: compact_id("node", &source_key),
                target: compact_id("node", &target_key),
                relationship: GraphRelationship::ObservedTransition,
                method: ScoreMethod::Statistical,
                metric: Some(WeightedEdgeMetric {
                    value: aggregate.value,
                    unit: options.metric.unit().to_owned(),
                    denominator,
                    share: aggregate.value / denominator,
                    sample_size: aggregate.samples,
                    window: options
                        .window
                        .clone()
                        .unwrap_or_else(|| trace.window.clone()),
                }),
                provenance: vec![GraphProvenance {
                    source: "harness-lens-sdk".to_owned(),
                    method: ScoreMethod::Statistical,
                    evidence_ids,
                    total_evidence: aggregate.evidence_ids.len(),
                    location: aggregate.target.location.clone(),
                }],
            });
        }
    } else if availability == GraphAvailability::InsufficientEvidence {
        if let Some(observation) = trace.observations.first() {
            let node = graph_node(0, observation);
            node_records.insert(node.id.clone(), node);
        }
    }

    let completeness_reasons = reasons
        .into_iter()
        .map(|(code, count)| CompletenessReason {
            code,
            count: Some(count),
        })
        .collect::<Vec<_>>();
    let graph = RelationshipGraph {
        schema_version: RELATIONSHIP_GRAPH_SCHEMA_VERSION,
        kind: GraphKind::ObservedFlow,
        method: ScoreMethod::Statistical,
        availability,
        completeness: EvidenceCompleteness {
            complete: completeness_reasons.is_empty(),
            reasons: completeness_reasons,
        },
        limits: GraphLimits {
            max_nodes: options.max_nodes,
            max_edges: options.max_edges,
            max_hops: options.max_hops,
        },
        filters: GraphFilters {
            root: options.root.clone(),
            window: options.window.clone(),
            categories: options.categories.iter().cloned().collect(),
            statuses: options.statuses.iter().copied().collect(),
            minimum_share: options.minimum_share,
            metric_unit: options.metric.unit().to_owned(),
        },
        nodes: node_records.into_values().collect(),
        edges: edge_records,
    }
    .canonicalize();
    graph.validate().map_err(|_| FlowBuildError::InvalidGraph)?;
    Ok(graph)
}

/// Builds an observed-flow graph and aligned bounded per-turn token timeline.
pub fn build_observed_flow_projection(
    trace: &ActionTrace,
    options: &ObservedFlowOptions,
    max_turns: usize,
) -> Result<ObservedFlowProjection, FlowBuildError> {
    if max_turns == 0 || max_turns > ABSOLUTE_MAX_FLOW_TURNS {
        return Err(FlowBuildError::InvalidOptions);
    }
    let graph = build_observed_flow(trace, options)?;
    let token_timeline = build_token_timeline(trace, options, &graph, max_turns)?;
    Ok(ObservedFlowProjection {
        graph,
        token_timeline,
    })
}

fn build_token_timeline(
    trace: &ActionTrace,
    options: &ObservedFlowOptions,
    graph: &RelationshipGraph,
    max_turns: usize,
) -> Result<ObservedTokenTimeline, FlowBuildError> {
    trace
        .validate(trace.observations.len().max(1))
        .map_err(|_| FlowBuildError::InvalidTrace)?;
    let mut reasons = graph
        .completeness
        .reasons
        .iter()
        .map(|reason| (reason.code.clone(), reason.count.unwrap_or(1)))
        .collect::<BTreeMap<_, _>>();
    let mut grouped = BTreeMap::<&str, Vec<&ActionObservation>>::new();
    for observation in &trace.observations {
        grouped
            .entry(&observation.session_id)
            .or_default()
            .push(observation);
    }
    let selected_turn_ids = selected_turn_ids(&grouped, options, graph);
    let mut turns = Vec::new();
    for observations in grouped.values() {
        for (layer, observation) in observations.iter().enumerate() {
            if !selected_turn_ids.contains(observation.id.as_str()) {
                continue;
            }
            turns.push(ObservedFlowTurn {
                id: observation.id.clone(),
                session_id: observation.session_id.clone(),
                sequence: observation.sequence,
                layer,
                observed_at: observation.observed_at.clone(),
                action: observation.action.clone(),
                status: observation.status,
                token_usage: observation.token_usage,
                cost: observation.cost.clone(),
                location: observation.location.clone(),
            });
        }
    }
    let total_turns = turns.len();
    let missing_tokens = turns
        .iter()
        .filter(|turn| turn.token_usage.is_none())
        .count();
    if missing_tokens > 0 {
        *reasons.entry("missing_token_usage".to_owned()).or_default() += missing_tokens;
    }
    if turns.len() > max_turns {
        let omitted = turns.len() - max_turns;
        turns.truncate(max_turns);
        *reasons.entry("truncated_turns".to_owned()).or_default() += omitted;
    }
    let sample_size = turns
        .iter()
        .filter(|turn| turn.token_usage.is_some())
        .count();
    let availability = if sample_size > 0 {
        GraphAvailability::Ready
    } else if total_turns == 0 && graph.availability != GraphAvailability::Unavailable {
        GraphAvailability::Empty
    } else {
        GraphAvailability::Unavailable
    };
    let completeness_reasons = reasons
        .into_iter()
        .map(|(code, count)| CompletenessReason {
            code,
            count: Some(count),
        })
        .collect::<Vec<_>>();
    Ok(ObservedTokenTimeline {
        method: ScoreMethod::Statistical,
        availability,
        completeness: EvidenceCompleteness {
            complete: completeness_reasons.is_empty(),
            reasons: completeness_reasons,
        },
        max_turns,
        total_turns,
        sample_size,
        unit: "tokens".to_owned(),
        turns,
    })
}

fn selected_turn_ids<'a>(
    grouped: &BTreeMap<&str, Vec<&'a ActionObservation>>,
    options: &ObservedFlowOptions,
    graph: &RelationshipGraph,
) -> BTreeSet<&'a str> {
    let graph_nodes = graph
        .nodes
        .iter()
        .filter_map(|node| {
            node.layer
                .map(|layer| (node.id.as_str(), (layer, node.logical_id.as_str())))
        })
        .collect::<BTreeMap<_, _>>();
    let selected_edges = graph
        .edges
        .iter()
        .filter_map(|edge| {
            let source = graph_nodes.get(edge.source.as_str())?;
            let target = graph_nodes.get(edge.target.as_str())?;
            Some((source.0, source.1, target.0, target.1))
        })
        .collect::<BTreeSet<_>>();
    let mut selected = BTreeSet::new();
    let mut discarded_reasons = BTreeMap::new();
    for observations in grouped.values() {
        for (layer, pair) in observations.windows(2).enumerate() {
            let source = pair[0];
            let target = pair[1];
            if selected_edges.contains(&(
                layer,
                source.action.id.as_str(),
                layer + 1,
                target.action.id.as_str(),
            )) && matches_filters(source, target, options, &mut discarded_reasons)
                && metric_value(target, &options.metric, &mut discarded_reasons).is_some()
            {
                selected.insert(source.id.as_str());
                selected.insert(target.id.as_str());
            }
        }
    }
    if graph.availability == GraphAvailability::InsufficientEvidence {
        for evidence_id in graph
            .nodes
            .iter()
            .flat_map(|node| &node.provenance)
            .flat_map(|provenance| &provenance.evidence_ids)
        {
            if let Some(observation) = grouped
                .values()
                .flatten()
                .find(|observation| observation.id == *evidence_id)
            {
                selected.insert(observation.id.as_str());
            }
        }
    }
    selected
}

fn validate_options(options: &ObservedFlowOptions) -> Result<(), FlowBuildError> {
    if options.max_nodes == 0
        || options.max_nodes > 10_000
        || options.max_edges == 0
        || options.max_edges > 20_000
        || options.max_hops == 0
        || options.max_hops > 100
        || options
            .minimum_share
            .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
        || matches!(&options.metric, FlowMetric::Cost(unit) if unit.is_empty())
    {
        return Err(FlowBuildError::InvalidOptions);
    }
    Ok(())
}

fn matches_filters(
    source: &ActionObservation,
    target: &ActionObservation,
    options: &ObservedFlowOptions,
    reasons: &mut BTreeMap<String, usize>,
) -> bool {
    if !options.categories.is_empty() && !options.categories.contains(&target.action.category) {
        return false;
    }
    if !options.statuses.is_empty() && !options.statuses.contains(&target.status) {
        return false;
    }
    let Some(window) = &options.window else {
        return true;
    };
    let (Some(source_time), Some(target_time)) = (&source.observed_at, &target.observed_at) else {
        *reasons
            .entry("missing_filter_timestamp".to_owned())
            .or_default() += 1;
        return false;
    };
    source_time >= &window.start
        && source_time <= &window.end
        && target_time >= &window.start
        && target_time <= &window.end
}

fn metric_value(
    target: &ActionObservation,
    metric: &FlowMetric,
    reasons: &mut BTreeMap<String, usize>,
) -> Option<f64> {
    match metric {
        FlowMetric::Transitions | FlowMetric::DistinctSessions => Some(1.0),
        FlowMetric::DurationMicros => {
            target
                .duration_micros
                .map(|value| value as f64)
                .or_else(|| {
                    *reasons
                        .entry("missing_metric_value".to_owned())
                        .or_default() += 1;
                    None
                })
        }
        FlowMetric::Cost(unit) => target
            .cost
            .as_ref()
            .filter(|cost| &cost.unit == unit)
            .map(|cost| cost.value)
            .or_else(|| {
                *reasons
                    .entry("missing_metric_value".to_owned())
                    .or_default() += 1;
                None
            }),
    }
}

fn retain_reachable<'a>(
    aggregates: Vec<TransitionAggregate<'a>>,
    root: &str,
    max_hops: usize,
    reasons: &mut BTreeMap<String, usize>,
) -> Vec<TransitionAggregate<'a>> {
    let starts = aggregates
        .iter()
        .flat_map(|edge| {
            [
                (edge.source.action.id == root)
                    .then(|| node_key(edge.source_layer, &edge.source.action)),
                (edge.target.action.id == root)
                    .then(|| node_key(edge.target_layer, &edge.target.action)),
            ]
        })
        .flatten()
        .collect::<BTreeSet<_>>();
    if starts.is_empty() {
        *reasons.entry("root_not_found".to_owned()).or_default() += 1;
        return Vec::new();
    }
    let mut queue = starts
        .iter()
        .map(|node| (node.clone(), 0usize))
        .collect::<VecDeque<_>>();
    let mut reached = starts;
    while let Some((node, depth)) = queue.pop_front() {
        if depth >= max_hops {
            continue;
        }
        for edge in &aggregates {
            if node_key(edge.source_layer, &edge.source.action) != node {
                continue;
            }
            let target = node_key(edge.target_layer, &edge.target.action);
            if reached.insert(target.clone()) {
                queue.push_back((target, depth + 1));
            }
        }
    }
    aggregates
        .into_iter()
        .filter(|edge| {
            reached.contains(&node_key(edge.source_layer, &edge.source.action))
                && reached.contains(&node_key(edge.target_layer, &edge.target.action))
        })
        .collect()
}

fn graph_node(layer: usize, observation: &ActionObservation) -> GraphNode {
    GraphNode {
        id: compact_id("node", &node_key(layer, &observation.action)),
        logical_id: observation.action.id.clone(),
        label: observation.action.label.clone(),
        kind: GraphNodeKind::Action,
        layer: Some(layer),
        provenance: vec![GraphProvenance {
            source: "harness-lens-sdk".to_owned(),
            method: ScoreMethod::Deterministic,
            evidence_ids: vec![observation.id.clone()],
            total_evidence: 1,
            location: observation.location.clone(),
        }],
    }
}

fn aggregate_key(aggregate: &TransitionAggregate<'_>) -> (String, String) {
    (
        node_key(aggregate.source_layer, &aggregate.source.action),
        node_key(aggregate.target_layer, &aggregate.target.action),
    )
}

fn node_key(layer: usize, action: &ActionIdentity) -> String {
    format!("{layer}:{}", action.id)
}

fn compact_id(prefix: &str, value: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in value.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{prefix}:{hash:016x}")
}

fn insert_compact_identity(
    ids: &mut BTreeMap<String, String>,
    id: &str,
    expanded: &str,
) -> Result<(), FlowBuildError> {
    if ids
        .insert(id.to_owned(), expanded.to_owned())
        .is_some_and(|existing| existing != expanded)
    {
        return Err(FlowBuildError::IdentityCollision);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use harness_lens_core::{
        ACTION_TRACE_SCHEMA_VERSION, EvidenceDescriptor, ObservedCost, ObservedTokenUsage,
        RuntimeObservationStatus,
    };

    use super::*;

    fn observation(
        id: &str,
        session: &str,
        sequence: u64,
        action: &str,
        category: &str,
    ) -> ActionObservation {
        ActionObservation {
            id: id.to_owned(),
            session_id: session.to_owned(),
            sequence,
            observed_at: Some(format!("2026-09-13T00:00:0{sequence}Z")),
            action: ActionIdentity {
                id: action.to_owned(),
                label: action.to_owned(),
                category: category.to_owned(),
            },
            status: RuntimeObservationStatus::Success,
            duration_micros: Some(sequence * 10),
            retry_count: Some(0),
            cost: Some(ObservedCost {
                value: sequence as f64 / 100.0,
                unit: "USD".to_owned(),
                estimated: false,
            }),
            token_usage: Some(ObservedTokenUsage {
                input_tokens: Some(sequence * 60),
                output_tokens: Some(sequence * 40),
                cached_input_tokens: Some(sequence * 10),
                total_tokens: sequence * 100,
                estimated: false,
            }),
            error_class: None,
            model: None,
            asset_identity: None,
            revision: None,
            location: None,
            evidence: EvidenceDescriptor::default(),
        }
    }

    fn trace() -> ActionTrace {
        ActionTrace {
            schema_version: ACTION_TRACE_SCHEMA_VERSION,
            window: ObservationWindow {
                start: "2026-09-13T00:00:00Z".to_owned(),
                end: "2026-09-13T00:00:09Z".to_owned(),
            },
            completeness: EvidenceCompleteness::default(),
            observations: vec![
                observation("a1", "session-a", 1, "read", "io"),
                observation("a2", "session-a", 2, "write", "io"),
                observation("a3", "session-a", 3, "read", "io"),
                observation("b1", "session-b", 1, "read", "io"),
                observation("b2", "session-b", 2, "test", "validation"),
            ],
            total_observations: Some(5),
            next_cursor: None,
        }
    }

    #[test]
    fn cycles_become_repeated_logical_nodes_by_layer() {
        let graph = build_observed_flow(&trace(), &ObservedFlowOptions::default()).unwrap();
        assert_eq!(graph.availability, GraphAvailability::Ready);
        assert!(graph.edges.iter().all(|edge| {
            let metric = edge.metric.as_ref().unwrap();
            metric.unit == "transitions" && metric.denominator == 3.0
        }));
        let read = graph
            .nodes
            .iter()
            .filter(|node| node.logical_id == "read")
            .collect::<Vec<_>>();
        assert_eq!(read.len(), 2);
        assert_ne!(read[0].layer, read[1].layer);
        assert_eq!(
            serde_json::to_string(&graph).unwrap(),
            serde_json::to_string(
                &build_observed_flow(&trace(), &ObservedFlowOptions::default()).unwrap()
            )
            .unwrap()
        );
    }

    #[test]
    fn filters_recalculate_denominator_without_new_adjacency() {
        let mut options = ObservedFlowOptions::default();
        options.categories.insert("validation".to_owned());
        let graph = build_observed_flow(&trace(), &options).unwrap();
        assert_eq!(graph.edges.len(), 1);
        let metric = graph.edges[0].metric.as_ref().unwrap();
        assert_eq!(metric.value, 1.0);
        assert_eq!(metric.denominator, 1.0);
        assert_eq!(metric.share, 1.0);
    }

    #[test]
    fn missing_duration_is_not_coerced_to_zero() {
        let mut trace = trace();
        for observation in &mut trace.observations {
            observation.duration_micros = None;
        }
        let graph = build_observed_flow(
            &trace,
            &ObservedFlowOptions {
                metric: FlowMetric::DurationMicros,
                ..ObservedFlowOptions::default()
            },
        )
        .unwrap();
        assert_eq!(graph.availability, GraphAvailability::Empty);
        assert!(graph.edges.is_empty());
        assert!(
            graph
                .completeness
                .reasons
                .iter()
                .any(|reason| reason.code == "missing_metric_value")
        );
    }

    #[test]
    fn complete_empty_trace_is_distinct_from_missing_evidence() {
        let mut complete = trace();
        complete.observations.clear();
        complete.total_observations = Some(0);
        let graph = build_observed_flow(&complete, &ObservedFlowOptions::default()).unwrap();
        assert_eq!(graph.availability, GraphAvailability::Empty);

        complete.completeness = EvidenceCompleteness {
            complete: false,
            reasons: vec![CompletenessReason {
                code: "source_incompatible".to_owned(),
                count: Some(1),
            }],
        };
        let graph = build_observed_flow(&complete, &ObservedFlowOptions::default()).unwrap();
        assert_eq!(graph.availability, GraphAvailability::Unavailable);
    }

    #[test]
    fn bounds_are_deterministic_and_visible() {
        let graph = build_observed_flow(
            &trace(),
            &ObservedFlowOptions {
                max_nodes: 2,
                max_edges: 1,
                ..ObservedFlowOptions::default()
            },
        )
        .unwrap();
        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.edges.len(), 1);
        assert!(!graph.completeness.complete);
        assert!(
            graph
                .completeness
                .reasons
                .iter()
                .any(|reason| reason.code.starts_with("truncated_"))
        );
    }

    #[test]
    fn token_timeline_preserves_turn_order_cost_and_bounds() {
        let projection =
            build_observed_flow_projection(&trace(), &ObservedFlowOptions::default(), 3).unwrap();
        assert_eq!(
            projection.token_timeline.availability,
            GraphAvailability::Ready
        );
        assert_eq!(projection.token_timeline.total_turns, 5);
        assert_eq!(projection.token_timeline.sample_size, 3);
        assert_eq!(projection.token_timeline.turns.len(), 3);
        assert_eq!(projection.token_timeline.turns[0].id, "a1");
        assert_eq!(
            projection.token_timeline.turns[0]
                .token_usage
                .unwrap()
                .total_tokens,
            100
        );
        assert_eq!(
            projection.token_timeline.turns[0]
                .cost
                .as_ref()
                .map(|cost| cost.value),
            Some(0.01)
        );
        assert!(
            projection
                .token_timeline
                .completeness
                .reasons
                .iter()
                .any(|reason| reason.code == "truncated_turns" && reason.count == Some(2))
        );
    }

    #[test]
    fn token_timeline_keeps_both_ends_of_selected_transitions() {
        let mut options = ObservedFlowOptions::default();
        options.categories.insert("validation".to_owned());
        let projection = build_observed_flow_projection(&trace(), &options, 10).unwrap();

        assert_eq!(projection.graph.edges.len(), 1);
        assert_eq!(projection.token_timeline.total_turns, 2);
        assert_eq!(
            projection
                .token_timeline
                .turns
                .iter()
                .map(|turn| turn.id.as_str())
                .collect::<Vec<_>>(),
            vec!["b1", "b2"]
        );
    }

    #[test]
    fn missing_token_usage_remains_unavailable_not_zero() {
        let mut value = trace();
        for observation in &mut value.observations {
            observation.token_usage = None;
        }
        let projection =
            build_observed_flow_projection(&value, &ObservedFlowOptions::default(), 10).unwrap();
        assert_eq!(
            projection.token_timeline.availability,
            GraphAvailability::Unavailable
        );
        assert_eq!(projection.token_timeline.sample_size, 0);
        assert!(
            projection
                .token_timeline
                .turns
                .iter()
                .all(|turn| turn.token_usage.is_none())
        );
        assert!(
            projection
                .token_timeline
                .completeness
                .reasons
                .iter()
                .any(|reason| reason.code == "missing_token_usage" && reason.count == Some(5))
        );
    }
}
