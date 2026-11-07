// SPDX-License-Identifier: BSD-2-Clause

#include "layout.h"
#include <fluxsort.h>

void bench_fluxsort_u32(void *data, size_t n) {
  fluxsort_uint32(data, n, NULL);
}
void bench_fluxsort_u64(void *data, size_t n) {
  fluxsort_uint64(data, n, NULL);
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
void bench_fluxsort_f64(void *data, size_t n) { fluxsort_f64(data, n, NULL); }

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
void bench_fluxsort_row32(void *data, size_t n) {
  fluxsort_row32(data, n, NULL);
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
void bench_fluxsort_row64(void *data, size_t n) {
  fluxsort_row64(data, n, NULL);
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
void bench_fluxsort_record(void *data, size_t n) {
  fluxsort_record(data, n, NULL);
}
