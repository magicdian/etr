# Directory Structure

> Frontend directory rules for `etr`.

## Overview

No frontend directory exists yet.

When a Web UI is introduced, add its actual layout here in the same change. Do not create speculative docs that pretend a frontend tree already exists.

## Current Rule

- Backend-only changes should not create placeholder frontend app trees
- If a UI is introduced, prefer a clearly separated app directory rather than mixing client files into Rust crates

## Examples

- Current backend-only repository layout:
  `README.md`, `config/`, `crates/`, `docs/`, `ebpf/`
