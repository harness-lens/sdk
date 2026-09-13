> SPDX-License-Identifier: MPL-2.0
> Copyright © 2026 Cristian Camargo Filho

# Sanitized action traces and observed flow

SDK accepts schema-version-1 JSON snapshots through
`trace::normalize_trace_json`. Input is deny-by-default: unknown fields fail
parsing, so prompts, source content, tool arguments, tool output, transcripts,
stderr, credentials, and secrets have no accepted payload field.

Required observation fields are stable observation ID, session ID, sequence,
action ID, action label, category, and status. Timestamp, duration, retry count,
cost, per-turn token usage, model, harness identity, revision, and
workspace-relative source location remain optional. Token usage accepts an
explicit total plus optional input, output, and cached-input components, and
records whether any count was estimated. Cached input is a subset of input;
supplied components must agree with the total. Missing measurements remain
absent; they are never coerced to zero. Redacted fields must appear in
`redacted_fields`.

Normalization sorts by session, sequence, and observation ID. Ambiguous IDs or
session positions are dropped and recorded as incomplete evidence. Source and
SDK bounds, redaction, missing fields, incompatibility, and reordering remain
visible through content-free reason codes.

`observed_flow::build_observed_flow` forms edges only from adjacent original
events in each session. Filtering never joins events across an omitted action.
Category and status filters apply to destination actions. Window filters require
both endpoints. Root traversal is directed and hop-bounded.

One request selects exactly one width unit:

- `transitions`: adjacent transition count;
- `sessions`: distinct session count per transition;
- `microseconds`: destination-action duration;
- caller-declared cost unit: destination-action cost matching that unit.

Denominators are recalculated after semantic filters and minimum-share
selection, but before response truncation. A truncated graph therefore keeps
the full filtered denominator and reports missing nodes or edges explicitly.
Repeated logical actions receive one node per sequence layer, preserving cycles
without presenting static relationships as observed behavior.

`observed_flow::build_observed_flow_projection` pairs that graph with a bounded
per-turn timeline for renderer synchronization. Timeline turns use the same
canonical session and sequence ordering and retain only layers represented by
the bounded graph. The timeline method is statistical and exposes `sample_size`
as the number of returned turns carrying token evidence. Turns without token
usage remain explicit gaps; no bar or aggregate may treat them as zero. The
response reports the pre-bound `total_turns`, applied `max_turns`, and
content-free completeness reasons such as `missing_token_usage` and
`truncated_turns`.
