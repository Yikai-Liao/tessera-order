// SPDX-License-Identifier: BSD-2-Clause

#include "layout.h"
#include <fluxsort.h>

void bench_fluxsort_u32(void *records, size_t len) {
  fluxsort_uint32(records, len, NULL);
}
void bench_fluxsort_u64(void *records, size_t len) {
  fluxsort_uint64(records, len, NULL);
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

#include <fluxsort.c>
#undef QUAD_CACHE
#undef cmp
#undef FUNC
#undef VAR
void bench_fluxsort_f64(void *records, size_t len) { fluxsort_f64(records, len, NULL); }

#define VAR BenchRow32
#define FUNC(NAME) NAME##_row32
#define cmp(a, b) ((a)->key > (b)->key)
#define QUAD_CACHE 4294967295
#include <quadsort.c>

#include <fluxsort.c>
#undef QUAD_CACHE
#undef cmp
#undef FUNC
#undef VAR
void bench_fluxsort_row32(void *records, size_t len) {
  fluxsort_row32(records, len, NULL);
}

#define VAR BenchRow64
#define FUNC(NAME) NAME##_row64
#define cmp(a, b) ((a)->key > (b)->key)
#define QUAD_CACHE 4294967295
#include <quadsort.c>

#include <fluxsort.c>
#undef QUAD_CACHE
#undef cmp
#undef FUNC
#undef VAR
void bench_fluxsort_row64(void *records, size_t len) {
  fluxsort_row64(records, len, NULL);
}

#define VAR BenchRecord
#define FUNC(NAME) NAME##_record
#define cmp(a, b) ((a)->key > (b)->key)
#define QUAD_CACHE 4294967295
#include <quadsort.c>

#include <fluxsort.c>
#undef QUAD_CACHE
#undef cmp
#undef FUNC
#undef VAR
void bench_fluxsort_record(void *records, size_t len) {
  fluxsort_record(records, len, NULL);
}
