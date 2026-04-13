#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
BUNDLE_ROOT=$(cd -- "${SCRIPT_DIR}/../.." && pwd)

exec "${BUNDLE_ROOT}/bin/etrd" install "$@"
