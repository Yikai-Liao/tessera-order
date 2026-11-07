// SPDX-License-Identifier: BSD-2-Clause

#include "layout.h"
#include <crumsort.h>

void bench_crumsort_u32(void *records, size_t len) {
  crumsort_uint32(records, len, NULL);
}
void bench_crumsort_u64(void *records, size_t len) {
  crumsort_uint64(records, len, NULL);
}
static uint64_t float_key(double value) {
  uint64_t bits;
  memcpy(&bits, &value, sizeof(bits));
  return bits >> 63 ? ~bits : bits ^ (UINT64_C(1) << 63);
}

#define VAR double
#define FUNC(NAME) NAME##_f64
#define cmp(a, b) (float_key(*(a)) > float_key(*(b)))
#define QUAD_CACHE 4294967295
#include <quadsort.c>

#include <crumsort.c>
#undef QUAD_CACHE
#undef cmp
#undef FUNC
#undef VAR
void bench_crumsort_f64(void *records, size_t len) { crumsort_f64(records, len, NULL); }

#define VAR BenchRow32
#define FUNC(NAME) NAME##_row32
#define cmp(a, b) ((a)->key > (b)->key)
#define QUAD_CACHE 4294967295
#include <quadsort.c>

#include <crumsort.c>
#undef QUAD_CACHE
#undef cmp
#undef FUNC
#undef VAR
void bench_crumsort_row32(void *records, size_t len) {
  crumsort_row32(records, len, NULL);
}

#define VAR BenchRow64
#define FUNC(NAME) NAME##_row64
#define cmp(a, b) ((a)->key > (b)->key)
#define QUAD_CACHE 4294967295
#include <quadsort.c>

#include <crumsort.c>
#undef QUAD_CACHE
#undef cmp
#undef FUNC
#undef VAR
void bench_crumsort_row64(void *records, size_t len) {
  crumsort_row64(records, len, NULL);
}

#define VAR BenchRecord
#define FUNC(NAME) NAME##_record
#define cmp(a, b) ((a)->key > (b)->key)
#define QUAD_CACHE 4294967295
#include <quadsort.c>

#include <crumsort.c>
#undef QUAD_CACHE
#undef cmp
#undef FUNC
#undef VAR
void bench_crumsort_record(void *records, size_t len) {
  crumsort_record(records, len, NULL);
}
