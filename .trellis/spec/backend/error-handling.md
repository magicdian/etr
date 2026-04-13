# Error Handling

> How errors are handled in `etr`.

## Overview

Use typed errors inside library crates and convert them at the outer boundary.

- `etr-config` defines `ConfigError`
- `etr-control` defines `ControlPlaneError` and wraps config/data-plane failures
- `etrd` maps boundary errors into HTTP responses with status codes
- `anyhow` is reserved for the binary entrypoint and startup composition

## Error Types

- `ConfigError`:
  for file I/O, TOML parse failures, and rule validation failures
- `ControlPlaneError`:
  for reload/bootstrap failures while coordinating config and data plane
- `ApiError`:
  for HTTP response mapping in the daemon binary

Examples:

- `crates/etr-config/src/lib.rs`
- `crates/etr-control/src/runtime.rs`
- `crates/etrd/src/main.rs`

## Error Handling Patterns

- Validate once at the config boundary, before mutating runtime state
- Keep validation messages specific enough for operators to fix config quickly
- Return typed errors from library crates with `thiserror`
- Convert to transport-specific output only at the edge
- Prefer `?` propagation over manual matching when the target error type is clear

## API Error Responses

Current API errors use a small JSON shape:

```json
{ "error": "human readable message" }
```

Use `400 Bad Request` for config or reload failures caused by invalid operator input.
When future runtime faults need different handling, map them explicitly instead of collapsing everything into `500`.

## Common Mistakes

- Using `anyhow` in shared library crates where typed errors are more useful
- Logging an error and then swallowing it instead of returning it
- Mutating live runtime state before config validation completes
- Returning opaque messages like `"reload failed"` without rule context
