// SPDX-License-Identifier: BSD-2-Clause

#include "layout.h"
#include <array>
#include <cstddef>
#include <tuple>

using Sort = void (*)(void *, size_t);

struct Layout {
  size_t size;
  size_t alignment;
  size_t payload_offset;
};

// Record suffix, C representation, payload offset. Order is the Rust NativeType
// ABI.
#define NATIVE_RECORDS(X)                                                      \
  X(u32, uint32_t, 0)                                                          \
  X(u64, uint64_t, 0)                                                          \
  X(f64, double, 0)                                                            \
  X(row32, BenchRow32, offsetof(BenchRow32, row))                              \
  X(row64, BenchRow64, offsetof(BenchRow64, row))                              \
  X(record, BenchRecord, offsetof(BenchRecord, payload))

// Row order is checked against Rust's NativeAlgorithm ABI.
#define NATIVE_ALGORITHMS(X)                                                   \
  X(clausecker)                                                                \
  X(fluxsort)                                                                  \
  X(ips2ra)                                                                    \
  X(ips4o)                                                                     \
  X(vqsort)                                                                    \
  X(x86simdsort)                                                               \
  X(crumsort)                                                                  \
  X(duckdb)

// Supported algorithm/record pairs. One entry generates both the C declaration
// and the typed binding; omitted pairs have no kernel.
#define NATIVE_KERNELS(X)                                                      \
  X(clausecker, row32)                                                         \
  X(fluxsort, u32)                                                             \
  X(fluxsort, u64)                                                             \
  X(fluxsort, f64)                                                             \
  X(fluxsort, row32)                                                           \
  X(fluxsort, row64)                                                           \
  X(fluxsort, record)                                                          \
  X(ips2ra, u32)                                                               \
  X(ips2ra, u64)                                                               \
  X(ips2ra, f64)                                                               \
  X(ips2ra, row32)                                                             \
  X(ips2ra, row64)                                                             \
  X(ips2ra, record)                                                            \
  X(ips4o, u32)                                                                \
  X(ips4o, u64)                                                                \
  X(ips4o, f64)                                                                \
  X(ips4o, row32)                                                              \
  X(ips4o, row64)                                                              \
  X(ips4o, record)                                                             \
  X(vqsort, u32)                                                               \
  X(vqsort, u64)                                                               \
  X(x86simdsort, u32)                                                          \
  X(x86simdsort, u64)                                                          \
  X(crumsort, u32)                                                             \
  X(crumsort, u64)                                                             \
  X(crumsort, f64)                                                             \
  X(crumsort, row32)                                                           \
  X(crumsort, row64)                                                           \
  X(crumsort, record)                                                          \
  X(duckdb, u32)                                                               \
  X(duckdb, u64)                                                               \
  X(duckdb, row32)                                                             \
  X(duckdb, row64)                                                             \
  X(duckdb, record)

namespace records {
#define RECORD(NAME, TYPE, OFFSET)                                             \
  struct NAME {                                                                \
    static constexpr Layout layout{sizeof(TYPE), alignof(TYPE), OFFSET};       \
  };
NATIVE_RECORDS(RECORD)
#undef RECORD
} // namespace records

namespace algorithms {
#define ALGORITHM(NAME)                                                        \
  struct NAME {                                                                \
    static constexpr auto name = #NAME;                                        \
  };
NATIVE_ALGORITHMS(ALGORITHM)
#undef ALGORITHM
} // namespace algorithms

template <typename Algorithm, typename Record> constexpr Sort kernel = nullptr;

#define KERNEL(ALGORITHM, RECORD)                                              \
  extern "C" void bench_##ALGORITHM##_##RECORD(void *, size_t);                \
  template <>                                                                  \
  constexpr Sort kernel<algorithms::ALGORITHM, records::RECORD> =              \
      bench_##ALGORITHM##_##RECORD;
NATIVE_KERNELS(KERNEL)
#undef KERNEL

#define RECORD_TAG(NAME, TYPE, OFFSET) records::NAME{},
static constexpr auto record_types = std::tuple{NATIVE_RECORDS(RECORD_TAG)};
#undef RECORD_TAG
#define ALGORITHM_TAG(NAME) algorithms::NAME{},
static constexpr auto algorithm_types =
    std::tuple{NATIVE_ALGORITHMS(ALGORITHM_TAG)};
#undef ALGORITHM_TAG
#undef NATIVE_KERNELS
#undef NATIVE_ALGORITHMS
#undef NATIVE_RECORDS

// std::apply preserves the type-list order for both layout rows and kernel
// columns.
static constexpr auto layouts = std::apply(
    [](auto... record) { return std::array{decltype(record)::layout...}; },
    record_types);

struct RegistryRow {
  const char *name;
  std::array<Sort, layouts.size()> kernels;
};

template <typename Algorithm> constexpr RegistryRow registry_row(Algorithm) {
  return {Algorithm::name,
          std::apply(
              [](auto... record) {
                return std::array{kernel<Algorithm, decltype(record)>...};
              },
              record_types)};
}

static constexpr auto registry = std::apply(
    [](auto... algorithm) { return std::array{registry_row(algorithm)...}; },
    algorithm_types);

extern "C" Sort bench_kernel(unsigned algorithm, unsigned type) noexcept {
  if (algorithm >= registry.size() || type >= layouts.size())
    return nullptr;
  return registry[algorithm].kernels[type];
}

extern "C" const char *bench_algorithm_name(unsigned algorithm) noexcept {
  return algorithm < registry.size() ? registry[algorithm].name : nullptr;
}

extern "C" size_t bench_layout(unsigned type, unsigned field) noexcept {
  if (type >= layouts.size())
    return 0;
  const auto &layout = layouts[type];
  switch (field) {
  case 0:
    return layout.size;
  case 1:
    return layout.alignment;
  case 2:
    return layout.payload_offset;
  default:
    return 0;
  }
}
