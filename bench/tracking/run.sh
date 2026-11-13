#!/bin/sh
# SPDX-License-Identifier: BSD-2-Clause
set -eu
cd /app
# Inherited harness control variables must not turn a measured run into a listing.
unset TESSERA_BENCH_LIST TESSERA_BENCH_PAIRED TESSERA_BENCH_FIXED_SORTS
test_mode=false
[ "${1:-}" != --test ] || test_mode=true
# Expand the actual allowed set; never assume the runner owns CPU zero.
cpus=$(awk '/^Cpus_allowed_list:/ {n=split($2,parts,","); for(i=1;i<=n;i++) {split(parts[i],r,"-"); end=r[2]==""?r[1]:r[2]; for(j=r[1];j<=end;j++) print j}}' /proc/self/status)
# No silent fallback: changing these controls changes the measurement.
"$test_mode" || setarch x86_64 -R true
# Each mode runs on exactly its advertised number of allowed CPUs.
for mode in $(awk -F/ '{print $NF}' cases.ids | sort -u); do
  case "$mode" in api) threads=1;; par-[1248]) threads=${mode#par-};; *) echo 'Invalid mode' >&2; exit 1;; esac
  export TESSERA_BENCH_IDS="$(awk -F/ -v mode="$mode" '$NF == mode' cases.ids)"
  if "$test_mode"; then ./benchmark --test; continue; fi
  [ "$(printf '%s\n' "$cpus" | wc -l)" -ge "$threads" ] || { echo 'Insufficient allowed CPUs' >&2; exit 1; }
  taskset -c "$(printf '%s\n' "$cpus" | head -n "$threads" | paste -sd, -)" setarch x86_64 -R ./benchmark --bench --noplot
done
