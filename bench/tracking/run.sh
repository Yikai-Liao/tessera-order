#!/bin/sh
# SPDX-License-Identifier: BSD-2-Clause
set -eu
cd /app
case "${1:-}" in ''|*[!0-9]*) echo 'Expected shard index' >&2; exit 1;; esac
shard=$1
export TESSERA_BENCH_PLAN=/app/plan.json
algorithm=$(cat algorithm)
unset TESSERA_BENCH_LIST TESSERA_BENCH_PAIRED TESSERA_BENCH_FIXED_SORTS TESSERA_BENCH_SCRATCH
export TESSERA_BENCH_IDS="$(cat "$shard.ids")"
[ -n "$TESSERA_BENCH_IDS" ]
test_mode=false
[ "${2:-}" != --test ] || test_mode=true
cpus=$(awk '/^Cpus_allowed_list:/ {n=split($2,parts,","); for(i=1;i<=n;i++) {split(parts[i],r,"-"); end=r[2]==""?r[1]:r[2]; for(j=r[1];j<=end;j++) print j}}' /proc/self/status)
if ! "$test_mode"; then setarch x86_64 -R true; fi
if [ "$algorithm" = glidesort-sqrt ]; then
  if "$test_mode"; then ./memory --plan "$shard.memory.json" --out "memory-$shard"
  else taskset -c "$(printf '%s\n' "$cpus" | head -n 1)" setarch x86_64 -R ./memory --plan "$shard.memory.json" --out "memory-$shard"; fi
  export TESSERA_BENCH_SCRATCH="/app/memory-$shard"
fi
# Each phase owns exactly its advertised number of allowed CPUs; never assume CPU zero.
for mode in $(awk -F/ '{print $NF}' "$shard.ids" | sort -u); do
  threads=1
  case "$mode" in api) ;; par-[1248]) threads=${mode#par-};; *) echo 'Invalid mode' >&2; exit 1;; esac
  export TESSERA_BENCH_IDS="$(awk -F/ -v mode="$mode" '$NF == mode' "$shard.ids")"
  if "$test_mode"; then ./benchmark --test
  else
    available=$(printf '%s\n' "$cpus" | wc -l)
    [ "$available" -ge "$threads" ] || { echo 'Insufficient allowed CPUs' >&2; exit 1; }
    cpu=$(printf '%s\n' "$cpus" | head -n "$threads" | paste -sd, -)
    taskset -c "$cpu" setarch x86_64 -R ./benchmark --bench --noplot
  fi
done
