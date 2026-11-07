// SPDX-License-Identifier: BSD-2-Clause

#include <cstddef>
#include <cstdlib>
#include <malloc.h>
#include <new>

extern "C" {
void bench_allocation_change(ptrdiff_t);
void *__real_malloc(size_t);
void *__real_calloc(size_t, size_t);
void *__real_realloc(void *, size_t);
void __real_free(void *);
void *__real_aligned_alloc(size_t, size_t);
int __real_posix_memalign(void **, size_t, size_t);

static size_t bytes(void *p) { return p ? malloc_usable_size(p) : 0; }
void *__wrap_malloc(size_t n) {
  void *p = __real_malloc(n);
  bench_allocation_change(bytes(p));
  return p;
}
void *__wrap_calloc(size_t n, size_t width) {
  void *p = __real_calloc(n, width);
  bench_allocation_change(bytes(p));
  return p;
}
void __wrap_free(void *p) {
  bench_allocation_change(-static_cast<ptrdiff_t>(bytes(p)));
  __real_free(p);
}
void *__wrap_realloc(void *p, size_t n) {
  const size_t old = bytes(p);
  void *q = __real_realloc(p, n);
  if (q || !n)
    bench_allocation_change(static_cast<ptrdiff_t>(bytes(q)) -
                            static_cast<ptrdiff_t>(old));
  return q;
}
void *__wrap_aligned_alloc(size_t alignment, size_t n) {
  void *p = __real_aligned_alloc(alignment, n);
  bench_allocation_change(bytes(p));
  return p;
}
int __wrap_posix_memalign(void **p, size_t alignment, size_t n) {
  int result = __real_posix_memalign(p, alignment, n);
  if (!result)
    bench_allocation_change(bytes(*p));
  return result;
}
}
void *operator new(size_t n) {
  if (void *p = __wrap_malloc(n ? n : 1))
    return p;
  throw std::bad_alloc();
}
void *operator new[](size_t n) { return ::operator new(n); }
void operator delete(void *p) noexcept { __wrap_free(p); }
void operator delete[](void *p) noexcept { __wrap_free(p); }
void operator delete(void *p, size_t) noexcept { __wrap_free(p); }
void operator delete[](void *p, size_t) noexcept { __wrap_free(p); }
void *operator new(size_t n, std::align_val_t alignment) {
  void *p = nullptr;
  if (__wrap_posix_memalign(&p, static_cast<size_t>(alignment), n ? n : 1))
    throw std::bad_alloc();
  return p;
}
void *operator new[](size_t n, std::align_val_t a) {
  return ::operator new(n, a);
}
void operator delete(void *p, std::align_val_t) noexcept { __wrap_free(p); }
void operator delete[](void *p, std::align_val_t) noexcept { __wrap_free(p); }
void operator delete(void *p, size_t, std::align_val_t) noexcept {
  __wrap_free(p);
}
void operator delete[](void *p, size_t, std::align_val_t) noexcept {
  __wrap_free(p);
}

extern "C" size_t bench_native_memory_probe() {
  void *a = ::operator new(12345);
  void *b = __wrap_malloc(12345);
  const size_t n = malloc_usable_size(a) + malloc_usable_size(b);
  static_cast<volatile unsigned char *>(a)[0] = 1;
  static_cast<volatile unsigned char *>(b)[0] = 2;
  ::operator delete(a);
  __wrap_free(b);
  return n;
}
