# Frontend Development Guidelines

> Current frontend status for `etr`.

## Overview

There is no frontend application in the repository today.

The MVP explicitly defers Web UI work. The active product surface is a Rust daemon plus a local HTTP management API.

These frontend docs therefore describe the current reality:

- no frontend framework is approved yet
- no components, hooks, or client-side state modules exist yet
- any future Web UI task must update these docs in the same change that introduces the frontend

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Module organization and file layout | Deferred: no frontend code yet |
| [Component Guidelines](./component-guidelines.md) | Component patterns, props, composition | Deferred: no frontend code yet |
| [Hook Guidelines](./hook-guidelines.md) | Custom hooks, data fetching patterns | Deferred: no frontend code yet |
| [State Management](./state-management.md) | Local state, global state, server state | Deferred: no frontend code yet |
| [Quality Guidelines](./quality-guidelines.md) | Code standards, forbidden patterns | Deferred: no frontend code yet |
| [Type Safety](./type-safety.md) | Type patterns, validation | Deferred: no frontend code yet |

## Current Rule

Do not invent frontend conventions as if they already exist.
If a task adds Web UI code, establish the real stack and update these guides first.

**Language**: Keep frontend documentation in English.
