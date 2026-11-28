// SPDX-License-Identifier: BSD-2-Clause

package tessera_test

import (
	"encoding/binary"
	"encoding/hex"
	"slices"
	"testing"

	. "github.com/Yikai-Liao/tessera-order/go/tessera"
)

func goldenSeeds(f *testing.F) [][]byte {
	vectors := readOrderingVectors(f)
	if vectors == nil {
		// The module archive omits repository fixtures.
		return [][]byte{[]byte("b.......a"), []byte("a.......b"), []byte("b.......b"), []byte("a.......c.......b")}
	}
	var seeds [][]byte
	for _, vector := range vectors {
		data := make([]byte, 0, len(vector.Encoded)*8)
		for _, encoded := range vector.Encoded {
			raw, err := hex.DecodeString(encoded)
			if err != nil {
				f.Fatal(err)
			}
			// Fold both 128-bit words into the 64-bit fuzz key.
			var key uint64
			for _, b := range raw {
				key = (key<<8 | key>>56) ^ uint64(b)
			}
			data = binary.LittleEndian.AppendUint64(data, key)
		}
		seeds = append(seeds, data)
	}
	return seeds
}

func fuzzRows(data []byte, scale uint8) []testRow {
	if len(data) == 0 {
		return nil
	}
	words := min((len(data)-1)/8+1, 512)
	length := words
	// Some inputs should hit the block route without requiring a large seed.
	if scale&7 == 7 {
		length = min(words*17, 5000)
	}
	rows := make([]testRow, length)
	for i := range rows {
		start := (i % words) * 8
		var raw [8]byte
		copy(raw[:], data[start:])
		payload := i
		key := binary.LittleEndian.Uint64(raw[:])
		if scale&8 != 0 {
			key &= 127
		}
		rows[i] = testRow{key: key, id: i, payload: &payload}
	}
	return rows
}

func FuzzOrdering(f *testing.F) {
	for i, data := range goldenSeeds(f) {
		f.Add(data, uint8(i), uint16(i*19))
	}
	f.Add([]byte{}, uint8(0), uint16(0))
	f.Fuzz(func(t *testing.T, data []byte, scale uint8, pick uint16) {
		source := fuzzRows(data, scale)
		expected := slices.Clone(source)
		slices.SortStableFunc(expected, compareRows)
		actual := slices.Clone(source)
		SortByKey(actual, rowKey)
		if !slices.Equal(actual, expected) {
			t.Fatal("keyed sort differs from reference")
		}
		actual = slices.Clone(source)
		SortByCachedKey(actual, rowKey)
		if !slices.Equal(actual, expected) {
			t.Fatal("cached sort differs from reference")
		}
		indices := ArgsortByKey(source, rowKey)
		checkPermutation(t, indices, len(source))
		for i, j := range indices {
			if source[j] != expected[i] {
				t.Fatalf("wrong stable argsort position %d", i)
			}
		}
		if len(source) > 0 {
			rank := int(pick) % len(source)
			actual = slices.Clone(source)
			before, nth, after := SelectNthByKey(actual, rank, rowKey)
			verifySelection(t, actual, expected, rank, before, nth, after)
		}
	})
}

func FuzzChangingKeys(f *testing.F) {
	for i, data := range goldenSeeds(f) {
		f.Add(data, uint8(i))
	}
	f.Fuzz(func(t *testing.T, data []byte, mode uint8) {
		source := fuzzRows(data, mode)
		checkChangingKeys(t, source, int(mode%3), len(source)/2)
	})
}

func checkChangingKeys(t *testing.T, source []testRow, operation, rank int) {
	t.Helper()
	rows := slices.Clone(source)
	calls := 0
	const exhausted = "extractor call bound exceeded"
	key := func(row *testRow) uint64 {
		calls++
		if calls > 200*len(rows) {
			panic(exhausted)
		}
		return row.key ^ uint64(calls)*0x9e3779b97f4a7c15
	}
	var indices []int
	p := recoverPanic(func() {
		switch operation {
		case 0:
			SortByKey(rows, key)
		case 1:
			if len(rows) > 0 {
				SelectNthByKey(rows, rank, key)
			}
		case 2:
			indices = ArgsortByKey(rows, key)
		}
	})
	if p != nil {
		if operation != 2 || p != "tessera: inconsistent radix partition" {
			t.Fatalf("unexpected panic: %v", p)
		}
		return
	}
	checkRecordOwners(t, rows, source)
	if operation == 2 {
		if !slices.Equal(rows, source) {
			t.Fatal("argsort moved source on changing keys")
		}
		checkPermutation(t, indices, len(rows))
	}
}

func FuzzPanicPosition(f *testing.F) {
	for i, data := range goldenSeeds(f) {
		f.Add(data, uint8(i), uint16(i*7))
	}
	f.Fuzz(func(t *testing.T, data []byte, mode uint8, pick uint16) {
		source := fuzzRows(data, mode)
		if len(source) < 2 {
			return
		}
		rows := slices.Clone(source)
		calls := 0
		at := 1 + int(pick)%(len(rows)/2)
		marker := new(int)
		key := func(row *testRow) uint64 {
			calls++
			if calls == at {
				panic(marker)
			}
			return row.key
		}
		p := recoverPanic(func() {
			switch mode % 4 {
			case 0:
				SortByKey(rows, key)
			case 1:
				SelectNthByKey(rows, len(rows)/2, key)
			case 2:
				ArgsortByKey(rows, key)
			case 3:
				SortByCachedKey(rows, key)
			}
		})
		if p != marker || calls > at {
			t.Fatal("callback panic did not propagate")
		}
	})
}
