// SPDX-License-Identifier: BSD-2-Clause

#include "layout.h"
#include <cstdlib>
#include <hwy/contrib/sort/vqsort.h>
#include <hwy/targets.h>
#include <x86simdsort.h>

#define SHIMS(TYPE, NAME)                                                      \
  extern "C" void bench_vqsort_##NAME(void *data, size_t n) noexcept {         \
    hwy::VQSort(static_cast<TYPE *>(data), n, hwy::SortAscending());           \
  }                                                                            \
  extern "C" void bench_x86simdsort_##NAME(void *data, size_t n) noexcept {    \
    x86simdsort::qsort(static_cast<TYPE *>(data), n);                          \
  }
SHIMS(uint32_t, u32)
SHIMS(uint64_t, u64)
// Float kernels are excluded: their NaN and signed-zero order differs from
// total_cmp.

extern "C" const char *bench_vqsort_target() noexcept {
  const auto targets = hwy::SupportedTargets() & HWY_TARGETS;
  return hwy::TargetName(targets & -targets);
}
extern "C" const char *bench_x86simdsort_target() noexcept {
  __builtin_cpu_init();
  if (!std::getenv("XSS_DISABLE_AVX512") && __builtin_cpu_supports("avx512f") &&
      __builtin_cpu_supports("avx512dq") && __builtin_cpu_supports("avx512vl"))
    return "avx512_skx";
  if (__builtin_cpu_supports("avx2"))
    return "avx2";
  return "scalar";
}
