#!/usr/bin/env bash
# SPDX-License-Identifier: BSD-2-Clause
# Bootstrap the dependency-free checker from the trusted base checkout.
set -euo pipefail
exec node "$(dirname "$0")/check-dco.ts" "$@"
