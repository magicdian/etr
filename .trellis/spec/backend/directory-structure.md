# Directory Structure

> How backend code is organized in `etr`.

## Overview

This repository uses a small Rust workspace with explicit crate boundaries.
Keep transport concerns, config contracts, and runtime orchestration separate.

## Directory Layout

```text
config/
  etr.toml              # Example runtime config
crates/
  etr-types/            # Shared wire types for kernel/user-space coordination
  etr-config/           # Shared config schema and validation
  etr-control/          # Runtime state and data-plane abstraction
  etrd/                 # Daemon binary and HTTP management API
docs/
  architecture.md       # Current architecture notes
ebpf/
  etr-ebpf/             # Placeholder crate for future Aya eBPF programs
```

## Module Organization

- Put schema and validation at the boundary:
  `crates/etr-config/src/lib.rs`
- Put kernel/user-space shared map contracts in:
  `crates/etr-types/src/lib.rs`
- Put long-lived runtime orchestration in `etr-control`:
  `crates/etr-control/src/runtime.rs`
- Put backend-specific forwarding abstractions in `etr-control`:
  `crates/etr-control/src/dataplane.rs`
- Put Linux-specific Aya loading and map sync in:
  `crates/etr-control/src/linux.rs`
- Keep the binary crate thin:
  `crates/etrd/src/main.rs` should wire CLI, logging, HTTP routes, and shutdown
- Keep eBPF programs out of the daemon crate:
  kernel-side code belongs under `ebpf/`

## Naming Conventions

- Crate names use the `etr-*` prefix
- File names use `snake_case`
- Types use domain names, not framework names:
  `EtrConfig`, `ForwardRule`, `AppRuntime`, `TcDataPlane`
- Keep transport-specific names at the edge:
  `HealthResponse` and `ApiError` stay in `etrd`, not shared crates

## Examples

- `crates/etr-config/src/lib.rs`: good example of boundary-owned types
- `crates/etr-types/src/lib.rs`: good example of kernel/user-space shared contracts
- `crates/etr-control/src/runtime.rs`: good example of shared runtime state without HTTP coupling
- `crates/etrd/src/main.rs`: good example of thin entrypoint composition
