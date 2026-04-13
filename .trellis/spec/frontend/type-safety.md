# Type Safety

> Frontend type-safety rules for `etr`.

## Overview

No TypeScript or frontend runtime validation stack exists yet.

## Current Rule

- Do not invent shared frontend types until a frontend package exists
- If a Web UI is added later, its API-facing types should be generated or derived from the management API contract where possible, rather than being hand-copied in multiple places
