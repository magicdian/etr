# State Management

> Frontend state management rules for `etr`.

## Overview

There is no frontend state layer yet.

The current management surface is a local HTTP API served by `etrd`, not a browser client.

## Current Rule

- Do not pre-select a frontend state library before a UI exists
- If a Web UI is introduced, document how it consumes the existing management API and how config snapshots are cached or refreshed
