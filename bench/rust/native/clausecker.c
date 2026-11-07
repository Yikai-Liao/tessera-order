// SPDX-License-Identifier: BSD-2-Clause

#include <stddef.h>

#include "radixsort.h"
int always_supported(void) { return 1; }
void bench_clausecker_row32(void *data, size_t n) {
  permuted_impl.full_sort(data, n);
}
