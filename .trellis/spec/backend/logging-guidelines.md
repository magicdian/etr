# Logging Guidelines

> How logging is done in `etr`.

## Overview

Use `tracing` with structured fields.

Current setup lives in `crates/etrd/src/main.rs` and uses `tracing-subscriber` with an environment filter.

## Log Levels

- `info`:
  daemon startup, management API bind, successful config apply, reload success
- `warn`:
  use when the daemon recovers or falls back but operator attention may still be useful
- `error`:
  request failures, startup failures, and unexpected shutdown or signal issues
- `debug`:
  reserve for future detailed rule diffing or per-flow diagnostics

## Structured Logging

- Prefer key/value fields over sentence-only logs
- Include stable identifiers when possible:
  `node`, `config_path`, `backend`, `rules`, `listen`
- Keep messages short and action-oriented
- Do not log entire config payloads by default

Examples:

- `crates/etrd/src/main.rs`
- `crates/etr-control/src/dataplane.rs`

## What to Log

- startup configuration summary
- management API listen address
- successful config apply counts
- reload failures with actionable error messages
- data-plane backend identity such as `tc-stub` today and `tc-aya` later

## What NOT to Log

- future secrets or API tokens if management auth is added
- full operator config files unless a debug workflow explicitly opts in
- packet payloads or high-volume per-packet logs
