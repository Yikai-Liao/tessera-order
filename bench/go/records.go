// SPDX-License-Identifier: BSD-2-Clause

package main

import (
	"encoding/binary"
	"fmt"
	"math"
	"unsafe"

	"github.com/Yikai-Liao/tessera-order/go/tessera"
)

type row32 struct{ key, index uint32 }
type row64 struct {
	key   uint64
	index uint32
	_     uint32
}
type wide struct {
	key     uint64
	payload [120]byte
}
type owned struct {
	key     uint64
	payload string
	_       [8]byte
}

type records[T comparable, K any] struct {
	values []T
	key    func(*T) K
	order  func(T) word
	append func([]byte, T) []byte
	direct directCalls[T]
	owned  bool
}

func materialize[T any](c inputCase, probe bool, width int, floats bool, convert func(word, int) T) []T {
	if probe {
		return nil
	}
	values := make([]T, c.Len)
	for i, raw := range drawKeys(c.Distribution, c.Len, width, floats) {
		values[i] = convert(raw, i)
	}
	return values
}

func prepare(c inputCase, req request, probe bool) (prepared, string) {
	u32 := func(w word, _ int) uint32 { return uint32(w.Lo) }
	u64 := func(w word, _ int) uint64 { return w.Lo }
	f64 := func(w word, _ int) float64 { return math.Float64frombits(w.Lo) }
	append64 := binary.LittleEndian.AppendUint64
	order64 := func(value uint64) word { return word{Lo: value} }
	orderFloat := func(value float64) word { return word{Lo: floatOrder(math.Float64bits(value))} }
	switch c.Type {
	case "u32":
		return direct(c, req, materialize(c, probe, 32, false, u32),
			func(v uint32) word { return order64(uint64(v)) }, func(b []byte, v uint32) []byte {
				return binary.LittleEndian.AppendUint32(b, v)
			})
	case "u64":
		return direct(c, req, materialize(c, probe, 64, false, u64), order64, append64)
	case "u128":
		return direct(c, req, materialize(c, probe, 128, false, func(w word, _ int) word { return w }),
			func(v word) word { return v }, func(b []byte, v word) []byte { return append64(append64(b, v.Lo), v.Hi) })
	case "f64":
		return direct(c, req, materialize(c, probe, 64, true, f64), orderFloat,
			func(b []byte, v float64) []byte { return append64(b, math.Float64bits(v)) })
	case "reverse_u64":
		values := materialize(c, probe, 64, false, func(w word, _ int) tessera.ReverseKey[uint64] { return tessera.Reverse(^w.Lo) })
		return direct(c, req, values,
			func(v tessera.ReverseKey[uint64]) word { return order64(^v.Value) },
			func(b []byte, v tessera.ReverseKey[uint64]) []byte { return append64(b, v.Value) })
	case "reverse_f64":
		values := materialize(c, probe, 64, true, func(w word, _ int) tessera.ReverseKey[float64] {
			return tessera.Reverse(math.Float64frombits(w.Lo ^ (1 << 63)))
		})
		return direct(c, req, values,
			func(v tessera.ReverseKey[float64]) word { return order64(^orderFloat(v.Value).Lo) },
			func(b []byte, v tessera.ReverseKey[float64]) []byte { return append64(b, math.Float64bits(v.Value)) })
	case "row_u32":
		values := materialize(c, probe, 32, false, func(w word, i int) row32 { return row32{uint32(w.Lo), uint32(i)} })
		return assemble(c, req, records[row32, uint32]{values: values, key: func(v *row32) uint32 { return v.key },
			order: func(v row32) word { return order64(uint64(v.key)) }, append: func(b []byte, v row32) []byte {
				return binary.LittleEndian.AppendUint32(binary.LittleEndian.AppendUint32(b, v.key), v.index)
			}})
	case "row_u64":
		values := materialize(c, probe, 64, false, func(w word, i int) row64 { return row64{key: w.Lo, index: uint32(i)} })
		return assemble(c, req, records[row64, uint64]{values: values, key: func(v *row64) uint64 { return v.key },
			order: func(v row64) word { return order64(v.key) }, append: func(b []byte, v row64) []byte {
				return binary.LittleEndian.AppendUint32(append64(b, v.key), v.index)
			}})
	case "record128", "rec128_u64":
		values := materialize(c, probe, 64, false, func(w word, i int) wide {
			value := wide{key: w.Lo}
			if c.Type == "record128" {
				for j := range value.payload {
					value.payload[j] = byte(i*31 + j)
				}
			}
			binary.LittleEndian.PutUint64(value.payload[:], uint64(i))
			return value
		})
		return assemble(c, req, records[wide, uint64]{values: values, key: func(v *wide) uint64 { return v.key },
			order: func(v wide) word { return order64(v.key) }, append: func(b []byte, v wide) []byte {
				return append(append64(b, v.key), v.payload[:]...)
			}})
	case "owned":
		values := materialize(c, probe, 64, false, func(w word, i int) owned {
			return owned{key: w.Lo, payload: fmt.Sprintf("%016x", i)}
		})
		return assemble(c, req, records[owned, uint64]{values: values, key: func(v *owned) uint64 { return v.key },
			order: func(v owned) word { return order64(v.key) }, owned: true, append: func(b []byte, v owned) []byte {
				return append(append64(append64(b, v.key), uint64(len(v.payload))), v.payload...)
			}})
	default:
		return prepared{}, "unsupported record type"
	}
}

func direct[T comparable](c inputCase, req request, values []T, order func(T) word, appendRecord func([]byte, T) []byte) (prepared, string) {
	return assemble(c, req, records[T, T]{values: values, order: order, append: appendRecord,
		direct: directCalls[T]{tessera.Sort[T], tessera.ParSort[T], tessera.Argsort[T], tessera.ParArgsort[T],
			func(v []T, i int) { tessera.SelectNth(v, i) }},
	})
}

func recordSize[T any]() int {
	var zero T
	return int(unsafe.Sizeof(zero))
}
