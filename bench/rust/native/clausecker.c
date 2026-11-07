// SPDX-License-Identifier: BSD-2-Clause

#include <stddef.h>

#include "radixsort.h"
int always_supported(void) { return 1; }
void bench_clausecker_row32(void *records, size_t len) {
  permuted_impl.full_sort(records, len);
}
