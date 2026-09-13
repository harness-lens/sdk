> SPDX-License-Identifier: MPL-2.0
> Copyright © 2026 Cristian Camargo Filho

# Sanitized action traces and observed flow

SDK accepts schema-version-1 JSON snapshots through
`trace::normalize_trace_json`. Input is deny-by-default: unknown fields fail
parsing, so prompts, source content, tool arguments, tool output, transcripts,
stderr, credentials, and secrets have no accepted payload field.

Required observation fields are stable observation ID, session ID, sequence,
action ID, action label, category, and status. Timestamp, duration, retry count,
cost, model, harness identity, revision, and workspace-relative source location
remain optional. Missing measurements remain absent; they are never coerced to
zero. Redacted fields must appear in `redacted_fields`.

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
