#!/usr/bin/env bash
# SPDX-License-Identifier: BSD-2-Clause
set -euo pipefail
: "${TESSERA_CPU_LOCK:?Set TESSERA_CPU_LOCK to the existing common CPU lock}"
test -f "$TESSERA_CPU_LOCK" || { echo 'Existing common lock not found' >&2; exit 2; }
if [[ $# != 0 && $# != 2 ]]; then
  echo 'usage: replay.sh [exact-native-capture.tsv new-result.json]' >&2
  exit 2
fi
base=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
exec 9<>"$TESSERA_CPU_LOCK"
flock -s -w 30 9
printf 'ACQUIRED shared FD9 '; date -u +%FT%TZ
python3 -c 'import os; s=os.fstat(9); print("device=%s inode=%s" % (s.st_dev,s.st_ino))'
release() {
  local result=$?
  flock -u 9
  printf 'RELEASE shared FD9 '; date -u +%FT%TZ
  exit "$result"
}
trap release EXIT
(cd "$base/src" && timeout 60s python3 -B -m unittest -v test_probe)
if [[ $# == 2 ]]; then
  python3 - "$1" <<'PY'
import hashlib
import sys
expected = '13766d5c1e52266dea18a93624eae49d1a6d40cb48b62b9499c111b3345b3dfe'
with open(sys.argv[1], 'rb') as source:
    actual = hashlib.sha256(source.read()).hexdigest()
if actual != expected:
    raise SystemExit('Native capture SHA mismatch; do not substitute an aligned graph')
PY
  timeout 60s python3 -B "$base/src/run_native_capture.py" "$1" "$2"
fi
