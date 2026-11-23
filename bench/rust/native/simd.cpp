// SPDX-License-Identifier: BSD-2-Clause

#include "layout.h"
#include <algorithm>
#include <cstdlib>
#include <hwy/contrib/sort/vqsort.h>
#include <hwy/targets.h>
#include <x86simdsort.h>

#define SHIMS(TYPE, NAME)                                                      \
  extern "C" void bench_vqsort_##NAME(void *records, size_t len) noexcept {         \
    hwy::VQSort(static_cast<TYPE *>(records), len, hwy::SortAscending());           \
  }                                                                            \
  extern "C" void bench_x86simdsort_##NAME(void *records, size_t len) noexcept {    \
    x86simdsort::qsort(static_cast<TYPE *>(records), len);                          \
  }
SHIMS(uint32_t, u32)
SHIMS(uint64_t, u64)
#undef SHIMS

template <typename T>
void stable_argsort(const T *keys, size_t len, size_t *output) {
  auto indices = x86simdsort::argsort(keys, len);
  for (auto first = indices.begin(); first != indices.end();) {
    const T key = keys[*first];
    auto end = first + 1;
    while (end != indices.end() && keys[*end] == key)
      ++end;
    if (end - first > 1)
      std::sort(first, end);
    first = end;
  }
  if (len != 0)
    std::copy(indices.begin(), indices.end(), output);
}

// Each operation returns zero on success and nonzero on a C++ exception.
#define OPERATIONS(TYPE, NAME)                                                 \
  extern "C" int bench_vqsort_select_##NAME(void *keys, size_t len,              \
                                            size_t index) noexcept {           \
    try {                                                                      \
      hwy::VQSelect(static_cast<TYPE *>(keys), len, index, hwy::SortAscending()); \
      return 0;                                                                \
    } catch (...) {                                                            \
      return 1;                                                                \
    }                                                                          \
  }                                                                            \
  extern "C" int bench_x86simdsort_select_##NAME(void *keys, size_t len,          \
                                                size_t index) noexcept {       \
    try {                                                                      \
      x86simdsort::qselect(static_cast<TYPE *>(keys), index, len);                \
      return 0;                                                                \
    } catch (...) {                                                            \
      return 1;                                                                \
    }                                                                          \
  }                                                                            \
  extern "C" int bench_x86simdsort_argsort_##NAME(                               \
      const void *keys, size_t len, size_t *output) noexcept {                   \
    try {                                                                      \
      stable_argsort(static_cast<const TYPE *>(keys), len, output);              \
      return 0;                                                                \
    } catch (...) {                                                            \
      return 1;                                                                \
    }                                                                          \
  }
OPERATIONS(uint32_t, u32)
OPERATIONS(uint64_t, u64)
#undef OPERATIONS

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
