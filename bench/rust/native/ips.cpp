// SPDX-License-Identifier: BSD-2-Clause

#include "layout.h"
#include <cstring>
#ifdef __clang__
#pragma clang diagnostic push
#if __has_warning("-Wmissing-template-arg-list-after-template-kw")
// Pinned IPS2Ra spells a deduced member-template call as `template classify(...)`.
#pragma clang diagnostic ignored "-Wmissing-template-arg-list-after-template-kw"
#endif
#endif
#include <ips2ra.hpp>
#ifdef __clang__
#pragma clang diagnostic pop
#endif
#include <ips4o.hpp>
#include <type_traits>

static uint64_t key(double value) {
  uint64_t bits;
  std::memcpy(&bits, &value, sizeof(bits));
  return bits >> 63 ? ~bits : bits ^ (uint64_t{1} << 63);
}
template <class T> static auto key(const T &value) {
  if constexpr (std::is_integral_v<T>)
    return value;
  else
    return value.key;
}
#define SHIMS(TYPE, NAME)                                                      \
  extern "C" void bench_ips2ra_##NAME(void *records, size_t len) noexcept {         \
    auto *values = static_cast<TYPE *>(records);                                  \
    ips2ra::sort(values, values + len, [](const TYPE &x) { return key(x); });    \
  }                                                                            \
  extern "C" void bench_ips4o_##NAME(void *records, size_t len) noexcept {          \
    auto *values = static_cast<TYPE *>(records);                                  \
    ips4o::sort(values, values + len,                                            \
                [](const TYPE &a, const TYPE &b) { return key(a) < key(b); }); \
  }
SHIMS(uint32_t, u32)
SHIMS(uint64_t, u64)
SHIMS(double, f64)
SHIMS(BenchRow32, row32)
SHIMS(BenchRow64, row64)
SHIMS(BenchRecord, record)
