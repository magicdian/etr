# Quality Guidelines

> Code quality standards for the `etr` backend.

## Overview

The current backend is small, so quality depends on preserving clean boundaries:

- config contract is explicit
- runtime mutations happen only after validation
- HTTP transport stays thin
- future TC/XDP work does not leak kernel-specific assumptions into config types

## Forbidden Patterns

- Parsing config ad hoc in multiple crates
- Using untyped `String` maps for rule data when a dedicated struct exists
- Hiding invalid operator input behind generic `"bad request"` errors
- Coupling HTTP handlers directly to future eBPF implementation details
- Expanding MVP scope silently:
  IPv6, multi-backend LB, ICMP, and distributed control are deferred

## Required Patterns

- Use `#[serde(deny_unknown_fields)]` on operator-facing config structures
- Validate config before mutating runtime state
- Keep one crate responsible for one boundary:
  schema in `etr-config`, kernel/user-space wire types in `etr-types`, orchestration in `etr-control`, HTTP in `etrd`
- Prefer typed enums for protocols, family, and SNAT mode
- Keep future expansion explicit in the model:
  `backends` is list-shaped even though MVP requires exactly one backend

## Testing Requirements

- Run `cargo fmt --all`
- Run `cargo check`
- Run `cargo test`
- Add unit tests for config validation when changing the config contract
- Add integration-style tests once reload behavior or TC wiring becomes more complex

## Code Review Checklist

- Does the change preserve the current MVP scope boundary?
- Is validation happening at the right boundary?
- Are error messages actionable for operators?
- Are logs structured and free of oversized config dumps?
- Does the change keep room for future IPv6, XDP, or multi-backend support without pretending they already exist?
