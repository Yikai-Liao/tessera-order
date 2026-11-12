#!/bin/sh
# SPDX-License-Identifier: BSD-2-Clause
set -eu
cd /app
export TESSERA_BENCH_PLAN=/app/plan.json
# Inherited harness control variables must not turn a measured run into a listing/test.
unset TESSERA_BENCH_LIST TESSERA_BENCH_PAIRED TESSERA_BENCH_FIXED_SORTS
# Expand the actual allowed set; never assume the runner owns CPU zero.
cpus=$(awk '/^Cpus_allowed_list:/ {n=split($2,parts,","); for(i=1;i<=n;i++) {split(parts[i],r,"-"); end=r[2]==""?r[1]:r[2]; for(j=r[1];j<=end;j++) print j}}' /proc/self/status)
set -- $cpus
[ "$#" -ge 4 ] || { echo 'Four allowed CPUs required' >&2; exit 1; }
serial=$1
parallel=$1,$2,$3,$4
# No silent fallback: changing these controls changes the measurement protocol.
setarch x86_64 -R true
for mode in api par-4; do
  export TESSERA_BENCH_IDS="$(cat "$mode.ids")"
  [ -n "$TESSERA_BENCH_IDS" ]
  cpu=$serial
  [ "$mode" = api ] || cpu=$parallel
  taskset -c "$cpu" setarch x86_64 -R ./benchmark --bench --noplot
 done
