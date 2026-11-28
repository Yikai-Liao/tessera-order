// SPDX-License-Identifier: BSD-2-Clause

// Package tessera provides stable radix sorting, argsort and selection for
// slices.
//
// A key is a fixed-width value with a defined order: the integer types
// (including int and uint), float32 and float64, bool, byte arrays of 1 to 16
// bytes, [Uint128], [Int128] and [CharKey], or [Reverse] and [Pair] of keys
// up to 128 encoded bits. Floats order as negative NaNs, -Inf, negative
// numbers, -0, +0, positive numbers, +Inf, positive NaNs; NaNs of one sign
// order by payload. Functions without ByKey treat each record as its own
// key. Every entry point checks the key type before reading records and
// panics for an unsupported one.
//
// Equal keys keep their input order. Key functions may run more than once per
// record unless a function says otherwise, and may observe records in any
// order. If a key function panics, the panic propagates and the slice
// contents are unspecified; a normal return, also under inconsistent keys,
// keeps every record exactly once.
//
// For fixed record and key widths, a serial call uses O(√n) workspace; the
// Par functions use O(P√n) with P workers. Argsort functions also allocate
// the returned indices, and [SortByCachedKey] caches one key per record.
package tessera
