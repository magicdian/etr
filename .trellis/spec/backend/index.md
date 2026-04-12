# Backend Development Guidelines

> Actual backend conventions for the current `etr` repository.

## Overview

`etr` currently ships a Rust backend only. The MVP is a single-host gateway daemon with:

- a shared config/validation crate
- a shared kernel/user-space wire-types crate
- a control-plane runtime crate
- a daemon binary exposing the management API
- an Aya-based eBPF crate for TC ingress/egress programs

There is no database in the current architecture.

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Module organization and file layout | Active |
| [Database Guidelines](./database-guidelines.md) | ORM patterns, queries, migrations | Not applicable yet |
| [Error Handling](./error-handling.md) | Error types, handling strategies | Active |
| [Linux TC Data Plane](./linux-tc-dataplane.md) | Executable NAT and forwarding contract for Linux TC | Active |
| [Quality Guidelines](./quality-guidelines.md) | Code standards, forbidden patterns | Active |
| [Logging Guidelines](./logging-guidelines.md) | Structured logging, log levels | Active |

## Pre-Development Checklist

Before editing backend code:

1. Read [Directory Structure](./directory-structure.md) for crate boundaries
2. Read [Error Handling](./error-handling.md) when adding config, runtime, or API paths
3. Read [Logging Guidelines](./logging-guidelines.md) before adding lifecycle or reload logs
4. Read [Linux TC Data Plane](./linux-tc-dataplane.md) when changing the Linux eBPF path, map layout, or NAT behavior
5. Read [Quality Guidelines](./quality-guidelines.md) before introducing new abstractions
6. If a task introduces persistence, first update [Database Guidelines](./database-guidelines.md)

## Current Backend Stack

- Language: Rust
- Async runtime: Tokio
- HTTP API: Axum
- Serialization: Serde + TOML
- Error types: `thiserror`
- Top-level app errors: `anyhow`
- Logging: `tracing` + `tracing-subscriber`

## Example Files

- `crates/etr-config/src/lib.rs`: config contract and validation boundary
- `crates/etr-types/src/lib.rs`: map key/value contract shared with eBPF code
- `crates/etr-control/src/runtime.rs`: control-plane runtime orchestration
- `crates/etr-control/src/dataplane.rs`: backend abstraction for TC/XDP evolution
- `crates/etr-control/src/linux.rs`: Linux TC loader and map synchronization
- `crates/etrd/src/main.rs`: daemon entrypoint and management API

**Language**: Keep backend documentation in English.
