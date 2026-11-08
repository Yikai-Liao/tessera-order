// SPDX-License-Identifier: BSD-2-Clause

#include "layout.h"
#include <functional>
#include <type_traits>

// Payloads move with their keys, but never participate in tie comparisons.
static bool operator<(const BenchRow32 &left, const BenchRow32 &right) {
  return left.key < right.key;
}
static bool operator<(const BenchRow64 &left, const BenchRow64 &right) {
  return left.key < right.key;
}
static bool operator<(const BenchRecord &left, const BenchRecord &right) {
  return left.key < right.key;
}

// The headers use these two release-mode DuckDB helpers, not engine services.
namespace duckdb {
template <class To, class From> static To UnsafeNumericCast(From value) {
  return static_cast<To>(value);
}
} // namespace duckdb
#define DUCKDB_EXPLICIT_FALLTHROUGH [[fallthrough]]
#include "ska_sort.hpp"
#undef DUCKDB_EXPLICIT_FALLTHROUGH

/** Supply raw, non-null integer keys to DuckDB's unmodified sorting headers. */
template <class T> struct ExtractKey {
  static constexpr bool narrow = std::is_same_v<T, uint32_t> ||
                                 std::is_same_v<T, BenchRow32>;
  bool requires_next_sort = false;
  size_t ska_sort_width = narrow ? 4 : 8;

  uint64_t operator()(const T &record) const {
    // DuckDB scans the high bytes of its first 64-bit key part.
    if constexpr (std::is_integral_v<T>)
      return uint64_t{record} << (narrow ? 32 : 0);
    else
      return uint64_t{record.key} << (narrow ? 32 : 0);
  }
  bool ByteIsSkippable(size_t) const { return false; }
  bool Interrupted() const { return false; }
};

/** Match SortedRun's Vergesort entry and Ska/pdqsort fallback, without SQL layout. */
template <class T> static void sort(T *records, size_t len) {
  if (len < 2)
    return;
  const ExtractKey<T> extractor;
  const auto fallback = [extractor](T *begin, T *end) {
    duckdb_ska_sort::ska_sort(begin, end, extractor);
  };
  duckdb_vergesort::vergesort(records, records + len, std::less<T>{}, fallback);
}

#define SHIM(TYPE, NAME)                                                        \
  extern "C" void bench_duckdb_##NAME(void *records, size_t len) noexcept {        \
    sort(static_cast<TYPE *>(records), len);                                    \
  }
SHIM(uint32_t, u32)
SHIM(uint64_t, u64)
SHIM(BenchRow32, row32)
SHIM(BenchRow64, row64)
SHIM(BenchRecord, record)
