// SPDX-License-Identifier: BSD-2-Clause

package tessera_test

import (
	"strings"
	"testing"

	. "github.com/Yikai-Liao/tessera-order/go/tessera"
	"github.com/Yikai-Liao/tessera-order/go/tessera/internal/engine"
)

// These fixtures exercise entry-time binding with the planned operation
// signatures while only the key model is available.
func directCodec[K any](_ []K) engine.Codec[K] {
	return engine.BindKey[K]()
}

func keyedEncoding[T, K any](records []T, key func(*T) K) Uint128 {
	codec := engine.BindKey[K]()
	return codec.Encode(key(&records[0]))
}

func TestKeyAPIFixture(t *testing.T) {
	type row struct {
		score float64
		id    uint32
	}
	records := []row{{score: 1.0, id: 23}}
	got := keyedEncoding(records, func(record *row) PairKey[ReverseKey[float64], uint32] {
		return Pair(Reverse(record.score), record.id)
	})
	want := Uint128{Hi: 0x400f_ffff, Lo: 0xffff_ffff_0000_0017}
	if got != want {
		t.Fatalf("keyed encoding = %#v, want %#v", got, want)
	}
}

func expectKeyPanic(t *testing.T, message string, action func()) {
	t.Helper()
	defer func() {
		value, ok := recover().(string)
		if !ok || !strings.Contains(value, message) {
			t.Errorf("panic = %q, want a message containing %q", value, message)
		}
	}()
	action()
}

func TestKeyBindingRejectsUnsupportedTypeBeforeCallback(t *testing.T) {
	type named uint32
	record := []uint64{42}
	called := false
	expectKeyPanic(t, "unsupported key type", func() {
		SortByKey(record, func(value *uint64) named {
			called = true
			*value = 0
			return named(*value)
		})
	})
	if called || record[0] != 42 {
		t.Fatal("unsupported key reached the callback")
	}
	expectKeyPanic(t, "unsupported key type", func() { Sort([]string{}) })
	expectKeyPanic(t, "unsupported key type", func() { Argsort([][0]byte{}) })
	expectKeyPanic(t, "unsupported key type", func() { Sort([][17]byte{}) })
	expectKeyPanic(t, "unsupported key type", func() { SelectNth([]any{uint64(1)}, 0) })
	expectKeyPanic(t, "unsupported key type", func() { Sort([]PairKey[uint8, string]{}) })
}

func TestKeyBindingRejectsWidthOver128(t *testing.T) {
	called := false
	expectKeyPanic(t, "exceeds 128 encoded bits", func() {
		SortByCachedKey([]int{1}, func(*int) PairKey[Uint128, bool] {
			called = true
			return Pair(Uint128{}, false)
		})
	})
	if called {
		t.Fatal("over-wide key reached the callback")
	}
}

func TestKeyCharRejectsNonScalars(t *testing.T) {
	for _, value := range []rune{-1, 0xd800, 0xdfff, 0x11_0000} {
		expectKeyPanic(t, "invalid Unicode scalar", func() { Char(value) })
	}
}
