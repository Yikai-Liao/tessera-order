// SPDX-License-Identifier: BSD-2-Clause

#ifndef TESSERA_BENCH_LAYOUT_H
#define TESSERA_BENCH_LAYOUT_H
#include <stddef.h>
#include <stdint.h>

typedef struct {
  uint32_t key;
  uint32_t row;
} BenchRow32;
typedef struct {
  uint64_t key;
  uint32_t row;
} BenchRow64;
typedef struct {
  uint64_t key;
  uint8_t payload[120];
} BenchRecord;
#endif
