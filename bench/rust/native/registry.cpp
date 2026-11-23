// SPDX-License-Identifier: BSD-2-Clause

#include "layout.h"
#include <array>
#include <cstddef>
#include <tuple>

using Sort = void (*)(void *, size_t);
// The record column determines the key type. Select takes a valid zero-based
// rank. Argsort fills len caller-owned indices from len read-only keys.
using Select = int (*)(void *, size_t, size_t) noexcept;
using Argsort = int (*)(const void *, size_t, size_t *) noexcept;

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

// Row order is checked against Rust's NativeImplementation ABI.
#define NATIVE_IMPLEMENTATIONS(X)                                                   \
  X(clausecker)                                                                \
  X(fluxsort)                                                                  \
  X(ips2ra)                                                                    \
  X(ips4o)                                                                     \
  X(vqsort)                                                                    \
  X(x86simdsort)                                                               \
  X(crumsort)                                                                  \
  X(duckdb)

// Supported implementation/record pairs. One entry generates both the C declaration
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

namespace implementations {
#define IMPLEMENTATION(NAME)                                                        \
  struct NAME {                                                                \
    static constexpr auto name = #NAME;                                        \
  };
NATIVE_IMPLEMENTATIONS(IMPLEMENTATION)
#undef IMPLEMENTATION
} // namespace implementations

template <typename Implementation, typename Record> constexpr Sort kernel = nullptr;

#define KERNEL(IMPLEMENTATION, RECORD)                                              \
  extern "C" void bench_##IMPLEMENTATION##_##RECORD(void *, size_t);                \
  template <>                                                                  \
  constexpr Sort kernel<implementations::IMPLEMENTATION, records::RECORD> =              \
      bench_##IMPLEMENTATION##_##RECORD;
NATIVE_KERNELS(KERNEL)
#undef KERNEL

template <typename Implementation, typename Record> constexpr Select select_kernel = nullptr;
template <typename Implementation, typename Record> constexpr Argsort argsort_kernel = nullptr;

#define SELECT_KERNEL(IMPLEMENTATION, RECORD)                                  \
  extern "C" int bench_##IMPLEMENTATION##_select_##RECORD(                     \
      void *, size_t, size_t) noexcept;                                        \
  template <>                                                                  \
  constexpr Select select_kernel<implementations::IMPLEMENTATION,             \
                                 records::RECORD> =                            \
      bench_##IMPLEMENTATION##_select_##RECORD;
SELECT_KERNEL(vqsort, u32)
SELECT_KERNEL(vqsort, u64)
SELECT_KERNEL(x86simdsort, u32)
SELECT_KERNEL(x86simdsort, u64)
#undef SELECT_KERNEL

#define ARGSORT_KERNEL(IMPLEMENTATION, RECORD)                                 \
  extern "C" int bench_##IMPLEMENTATION##_argsort_##RECORD(                    \
      const void *, size_t, size_t *) noexcept;                                 \
  template <>                                                                  \
  constexpr Argsort argsort_kernel<implementations::IMPLEMENTATION,            \
                                   records::RECORD> =                          \
      bench_##IMPLEMENTATION##_argsort_##RECORD;
ARGSORT_KERNEL(x86simdsort, u32)
ARGSORT_KERNEL(x86simdsort, u64)
#undef ARGSORT_KERNEL

#define RECORD_TAG(NAME, TYPE, OFFSET) records::NAME{},
static constexpr auto record_types = std::tuple{NATIVE_RECORDS(RECORD_TAG)};
#undef RECORD_TAG
#define IMPLEMENTATION_TAG(NAME) implementations::NAME{},
static constexpr auto implementation_types =
    std::tuple{NATIVE_IMPLEMENTATIONS(IMPLEMENTATION_TAG)};
#undef IMPLEMENTATION_TAG
#undef NATIVE_KERNELS
#undef NATIVE_IMPLEMENTATIONS
#undef NATIVE_RECORDS

// std::apply preserves the type-list order for both layout rows and kernel
// columns.
static constexpr auto layouts = std::apply(
    [](auto... record) { return std::array{decltype(record)::layout...}; },
    record_types);

struct RegistryRow {
  const char *name;
  std::array<Sort, layouts.size()> kernels;
  std::array<Select, layouts.size()> select;
  std::array<Argsort, layouts.size()> argsort;
};

template <typename Implementation> constexpr RegistryRow registry_row(Implementation) {
  return {Implementation::name,
          std::apply(
              [](auto... record) {
                return std::array{kernel<Implementation, decltype(record)>...};
              },
              record_types),
          std::apply(
              [](auto... record) {
                return std::array{select_kernel<Implementation, decltype(record)>...};
              },
              record_types),
          std::apply(
              [](auto... record) {
                return std::array{argsort_kernel<Implementation, decltype(record)>...};
              },
              record_types)};
}

static constexpr auto registry = std::apply(
    [](auto... implementation) { return std::array{registry_row(implementation)...}; },
    implementation_types);

extern "C" Sort bench_kernel(unsigned implementation, unsigned type) noexcept {
  if (implementation >= registry.size() || type >= layouts.size())
    return nullptr;
  return registry[implementation].kernels[type];
}

extern "C" Select bench_select_kernel(unsigned implementation, unsigned type) noexcept {
  if (implementation >= registry.size() || type >= layouts.size())
    return nullptr;
  return registry[implementation].select[type];
}

extern "C" Argsort bench_argsort_kernel(unsigned implementation, unsigned type) noexcept {
  if (implementation >= registry.size() || type >= layouts.size())
    return nullptr;
  return registry[implementation].argsort[type];
}

extern "C" const char *bench_implementation_name(unsigned implementation) noexcept {
  return implementation < registry.size() ? registry[implementation].name : nullptr;
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
