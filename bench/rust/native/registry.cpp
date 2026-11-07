// SPDX-License-Identifier: BSD-2-Clause

#include "layout.h"
#include <cstddef>
using Sort = void (*)(void *, size_t);
#define DECLARE(ALG, TYPE) extern "C" void bench_##ALG##_##TYPE(void *, size_t);
#define ALL(ALG)                                                               \
  DECLARE(ALG, u32)                                                            \
  DECLARE(ALG, u64) DECLARE(ALG, f64) DECLARE(ALG, row32) DECLARE(ALG, row64)  \
      DECLARE(ALG, record)
ALL(ips2ra)
ALL(ips4o) ALL(fluxsort) ALL(crumsort) DECLARE(vqsort, u32) DECLARE(vqsort, u64)
    DECLARE(x86simdsort, u32)
        DECLARE(x86simdsort,
                u64) extern "C" void bench_clausecker_row32(void *, size_t);
#define ROW(ALG)                                                               \
  {bench_##ALG##_u32,   bench_##ALG##_u64,   bench_##ALG##_f64,                \
   bench_##ALG##_row32, bench_##ALG##_row64, bench_##ALG##_record}
extern "C" Sort bench_kernel(unsigned algorithm, unsigned type) noexcept {
  static const Sort table[7][6] = {
      {nullptr, nullptr, nullptr, bench_clausecker_row32, nullptr, nullptr},
      ROW(fluxsort),
      ROW(ips2ra),
      ROW(ips4o),
      {bench_vqsort_u32, bench_vqsort_u64, nullptr, nullptr, nullptr, nullptr},
      {bench_x86simdsort_u32, bench_x86simdsort_u64, nullptr, nullptr, nullptr,
       nullptr},
      ROW(crumsort)};
  return algorithm < 7 && type < 6 ? table[algorithm][type] : nullptr;
}
extern "C" size_t bench_layout(unsigned type, unsigned field) noexcept {
  const size_t table[3][3] = {
      {sizeof(BenchRow32), alignof(BenchRow32), offsetof(BenchRow32, row)},
      {sizeof(BenchRow64), alignof(BenchRow64), offsetof(BenchRow64, row)},
      {sizeof(BenchRecord), alignof(BenchRecord),
       offsetof(BenchRecord, payload)}};
  return table[type][field];
}
