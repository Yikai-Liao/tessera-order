// SPDX-License-Identifier: BSD-2-Clause

package tessera_test

import (
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"math"
	"math/bits"
	"os"
	"testing"

	. "github.com/Yikai-Liao/tessera-order/go/tessera"
)

type orderingVector struct {
	Name        string          `json:"name"`
	WidthBits   uint            `json:"width_bits"`
	StorageBits uint            `json:"storage_bits"`
	VaryingMask string          `json:"varying_mask"`
	Input       []string        `json:"input"`
	Encoded     []string        `json:"encoded"`
	StableOrder []int           `json:"stable_order"`
	Argsort     []int           `json:"argsort"`
	Selections  []rankSelection `json:"selections"`
}

type rankSelection struct {
	Rank   int   `json:"rank"`
	Before []int `json:"before"`
	Nth    int   `json:"nth"`
	After  []int `json:"after"`
}

func readOrderingVectors(t *testing.T) []orderingVector {
	t.Helper()
	data, err := os.ReadFile("../../conformance/vectors/ordering.json")
	if err != nil {
		t.Fatal(err)
	}
	var file struct {
		Schema int              `json:"schema"`
		Cases  []orderingVector `json:"cases"`
	}
	if err := json.Unmarshal(data, &file); err != nil {
		t.Fatal(err)
	}
	if file.Schema != 1 || len(file.Cases) == 0 {
		t.Fatalf("invalid ordering vectors: schema %d, %d cases", file.Schema, len(file.Cases))
	}
	return file.Cases
}

func readKeyBits(t *testing.T, value string, width uint) Uint128 {
	t.Helper()
	if width == 0 || width > 128 || width%8 != 0 || uint(len(value))*4 != width {
		t.Fatalf("invalid %d-bit key %q", width, value)
	}
	var raw [16]byte
	if _, err := hex.Decode(raw[16-len(value)/2:], []byte(value)); err != nil {
		t.Fatal(err)
	}
	return Uint128{Hi: binary.BigEndian.Uint64(raw[:8]), Lo: binary.BigEndian.Uint64(raw[8:])}
}

func checkVector[K comparable](t *testing.T, vector orderingVector, fromRaw func(Uint128) K, same func(K, K) bool) {
	t.Helper()
	if same == nil {
		same = func(a, b K) bool { return a == b }
	}
	if len(vector.Input) != len(vector.Encoded) || len(vector.Input) == 0 {
		t.Fatal("expected one encoding per input")
	}
	keys := make([]K, len(vector.Input))
	for i, raw := range vector.Input {
		keys[i] = fromRaw(readKeyBits(t, raw, vector.WidthBits))
	}
	codec := directCodec(keys)
	if codec.Width != vector.WidthBits || codec.StorageBits != vector.StorageBits {
		t.Fatalf("width/storage = %d/%d, want %d/%d",
			codec.Width, codec.StorageBits, vector.WidthBits, vector.StorageBits)
	}
	mask := readKeyBits(t, vector.VaryingMask, vector.StorageBits)
	if codec.VaryingMask != mask {
		t.Fatalf("varying mask = %016x%016x, want %s", codec.VaryingMask.Hi, codec.VaryingMask.Lo, vector.VaryingMask)
	}
	for i, key := range keys {
		want := readKeyBits(t, vector.Encoded[i], vector.StorageBits)
		if got := codec.Encode(key); got != want {
			t.Errorf("input %s encodes to %016x%016x, want %s", vector.Input[i], got.Hi, got.Lo, vector.Encoded[i])
		}
		if got := codec.Decode(want); !same(got, key) {
			t.Errorf("decoding %s failed to recover the original key bits %s", vector.Encoded[i], vector.Input[i])
		}
	}
	// StableOrder, Argsort and Selections are loaded for milestone 2, when
	// the Go ordering operations can check these results.
}

type word interface {
	uint8 | uint16 | uint32 | uint64 | uint | uintptr | int8 | int16 | int32 | int64 | int
}

func checkWord[K word](t *testing.T, vector orderingVector) {
	t.Helper()
	checkVector(t, vector, func(raw Uint128) K { return K(raw.Lo) }, nil)
}

func rawKeyBytes(value Uint128) [16]byte {
	var raw [16]byte
	binary.BigEndian.PutUint64(raw[:8], value.Hi)
	binary.BigEndian.PutUint64(raw[8:], value.Lo)
	return raw
}

func rawFloat32(value Uint128) float32 { return math.Float32frombits(uint32(value.Lo)) }
func rawFloat64(value Uint128) float64 { return math.Float64frombits(value.Lo) }

func sameFloat32(a, b float32) bool { return math.Float32bits(a) == math.Float32bits(b) }
func sameFloat64(a, b float64) bool { return math.Float64bits(a) == math.Float64bits(b) }

func TestGoldenKeyEncodings(t *testing.T) {
	for _, vector := range readOrderingVectors(t) {
		t.Run(vector.Name, func(t *testing.T) {
			switch vector.Name {
			case "u8":
				checkWord[uint8](t, vector)
			case "u16", "nonzero_u16":
				checkWord[uint16](t, vector)
			case "u32":
				checkWord[uint32](t, vector)
				if bits.UintSize == 32 {
					checkWord[uint](t, vector)
					checkWord[uintptr](t, vector)
				}
			case "u64":
				checkWord[uint64](t, vector)
				if bits.UintSize == 64 {
					checkWord[uint](t, vector)
					checkWord[uintptr](t, vector)
				}
			case "u128":
				checkVector(t, vector, func(raw Uint128) Uint128 { return raw }, nil)
			case "i8":
				checkWord[int8](t, vector)
			case "i16", "nonzero_i16":
				checkWord[int16](t, vector)
			case "i32":
				checkWord[int32](t, vector)
				if bits.UintSize == 32 {
					checkWord[int](t, vector)
				}
			case "i64":
				checkWord[int64](t, vector)
				if bits.UintSize == 64 {
					checkWord[int](t, vector)
				}
			case "i128":
				checkVector(t, vector, func(raw Uint128) Int128 {
					return Int128{Hi: int64(raw.Hi), Lo: raw.Lo}
				}, nil)
			case "bool":
				checkVector(t, vector, func(raw Uint128) bool { return raw.Lo != 0 }, nil)
			case "char":
				checkVector(t, vector, func(raw Uint128) CharKey { return Char(rune(raw.Lo)) }, nil)
			case "f32":
				checkVector(t, vector, rawFloat32, sameFloat32)
			case "f64":
				checkVector(t, vector, rawFloat64, sameFloat64)
			case "bytes1":
				checkVector(t, vector, func(raw Uint128) [1]byte {
					value := rawKeyBytes(raw)
					return [1]byte(value[15:])
				}, nil)
			case "bytes2":
				checkVector(t, vector, func(raw Uint128) [2]byte {
					value := rawKeyBytes(raw)
					return [2]byte(value[14:])
				}, nil)
			case "bytes3":
				checkVector(t, vector, func(raw Uint128) [3]byte {
					value := rawKeyBytes(raw)
					return [3]byte(value[13:])
				}, nil)
			case "bytes4":
				checkVector(t, vector, func(raw Uint128) [4]byte {
					value := rawKeyBytes(raw)
					return [4]byte(value[12:])
				}, nil)
			case "bytes5":
				checkVector(t, vector, func(raw Uint128) [5]byte {
					value := rawKeyBytes(raw)
					return [5]byte(value[11:])
				}, nil)
			case "bytes6":
				checkVector(t, vector, func(raw Uint128) [6]byte {
					value := rawKeyBytes(raw)
					return [6]byte(value[10:])
				}, nil)
			case "bytes7":
				checkVector(t, vector, func(raw Uint128) [7]byte {
					value := rawKeyBytes(raw)
					return [7]byte(value[9:])
				}, nil)
			case "bytes8":
				checkVector(t, vector, func(raw Uint128) [8]byte {
					value := rawKeyBytes(raw)
					return [8]byte(value[8:])
				}, nil)
			case "bytes9":
				checkVector(t, vector, func(raw Uint128) [9]byte {
					value := rawKeyBytes(raw)
					return [9]byte(value[7:])
				}, nil)
			case "bytes10":
				checkVector(t, vector, func(raw Uint128) [10]byte {
					value := rawKeyBytes(raw)
					return [10]byte(value[6:])
				}, nil)
			case "bytes11":
				checkVector(t, vector, func(raw Uint128) [11]byte {
					value := rawKeyBytes(raw)
					return [11]byte(value[5:])
				}, nil)
			case "bytes12":
				checkVector(t, vector, func(raw Uint128) [12]byte {
					value := rawKeyBytes(raw)
					return [12]byte(value[4:])
				}, nil)
			case "bytes13":
				checkVector(t, vector, func(raw Uint128) [13]byte {
					value := rawKeyBytes(raw)
					return [13]byte(value[3:])
				}, nil)
			case "bytes14":
				checkVector(t, vector, func(raw Uint128) [14]byte {
					value := rawKeyBytes(raw)
					return [14]byte(value[2:])
				}, nil)
			case "bytes15":
				checkVector(t, vector, func(raw Uint128) [15]byte {
					value := rawKeyBytes(raw)
					return [15]byte(value[1:])
				}, nil)
			case "bytes16":
				checkVector(t, vector, func(raw Uint128) [16]byte {
					value := rawKeyBytes(raw)
					return [16]byte(value[0:])
				}, nil)
			case "reverse_f64":
				checkVector(t, vector, func(raw Uint128) ReverseKey[float64] {
					return Reverse(rawFloat64(raw))
				}, func(a, b ReverseKey[float64]) bool { return sameFloat64(a.Value, b.Value) })
			case "reverse_bytes3":
				checkVector(t, vector, func(raw Uint128) ReverseKey[[3]byte] {
					value := rawKeyBytes(raw)
					return Reverse([3]byte(value[13:]))
				}, nil)
			case "reverse_bool":
				checkVector(t, vector, func(raw Uint128) ReverseKey[bool] {
					return Reverse(raw.Lo != 0)
				}, nil)
			case "reverse_reverse_f32":
				type key = ReverseKey[ReverseKey[float32]]
				checkVector(t, vector, func(raw Uint128) key {
					return Reverse(Reverse(rawFloat32(raw)))
				}, func(a, b key) bool { return sameFloat32(a.Value.Value, b.Value.Value) })
			case "pair_cross64":
				type key = PairKey[bool, PairKey[float32, [6]byte]]
				checkVector(t, vector, func(raw Uint128) key {
					value := rawKeyBytes(raw)
					bits := binary.BigEndian.Uint32(value[6:10])
					return Pair(value[5] != 0, Pair(math.Float32frombits(bits), [6]byte(value[10:])))
				}, func(a, b key) bool {
					return a.First == b.First && sameFloat32(a.Second.First, b.Second.First) &&
						a.Second.Second == b.Second.Second
				})
			case "tuple16":
				checkVector(t, vector, tuple16, nil)
			default:
				t.Fatalf("unrecognized vector %q", vector.Name)
			}
		})
	}
}

type pair2 = PairKey[byte, byte]
type pair4 = PairKey[pair2, pair2]
type pair8 = PairKey[pair4, pair4]
type pair16 = PairKey[pair8, pair8]

func tuple16(raw Uint128) pair16 {
	bytes := rawKeyBytes(raw)
	first := Pair(Pair(Pair(bytes[0], bytes[1]), Pair(bytes[2], bytes[3])),
		Pair(Pair(bytes[4], bytes[5]), Pair(bytes[6], bytes[7])))
	second := Pair(Pair(Pair(bytes[8], bytes[9]), Pair(bytes[10], bytes[11])),
		Pair(Pair(bytes[12], bytes[13]), Pair(bytes[14], bytes[15])))
	return Pair(first, second)
}
