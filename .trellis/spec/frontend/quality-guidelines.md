# Quality Guidelines

> Frontend quality rules for `etr`.

## Overview

There is no frontend code yet, so current quality guidance is mostly about avoiding accidental scope creep.

## Forbidden Patterns

- Adding a speculative Web UI during backend-only work
- Adding placeholder frontend directories without a real stack decision
- Writing fake frontend guidelines that are not backed by code in the repo

## Required Patterns

- If a frontend is introduced, update all frontend spec files in the same change
- Base frontend conventions on actual committed code, not generic defaults

## Testing Requirements

Not applicable until a frontend exists.

## Code Review Checklist

- Does this change really need frontend code?
- If frontend code was added, were the frontend Trellis specs updated to reflect the real implementation?
