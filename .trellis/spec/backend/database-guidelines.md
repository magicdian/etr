# Database Guidelines

> Database patterns for `etr`.

## Overview

There is no database in the current MVP architecture.

The first implementation is a single-host gateway daemon whose source of truth is a static config file. Runtime state lives in memory and is pushed into the data plane.

## Current Rule

- Do not introduce a database casually
- If persistence becomes necessary, update this file in the same change
- Any storage introduction should explain:
  - why config file + in-memory state is no longer enough
  - what data is durable versus derived
  - how schema evolution and migration will work

## Query Patterns

Not applicable yet.

## Migrations

Not applicable yet.

## Naming Conventions

Not applicable yet.

## Common Mistakes

- Adding a database before the control-plane contract is stable
- Persisting derived forwarding state that can be rebuilt from config
