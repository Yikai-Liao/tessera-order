// SPDX-License-Identifier: BSD-2-Clause

package engine

import "math/bits"

type Uint128 struct {
	Hi, Lo uint64
}

type Int128 struct {
	Hi int64
	Lo uint64
}

func (value Uint128) bitLen() int {
	if value.Hi != 0 {
		return 64 + bits.Len64(value.Hi)
	}
	return bits.Len64(value.Lo)
}

func (value Uint128) lsh(shift uint) Uint128 {
	if shift >= 64 {
		return Uint128{Hi: value.Lo << (shift - 64)}
	}
	return Uint128{
		Hi: value.Hi<<shift | value.Lo>>(64-shift),
		Lo: value.Lo << shift,
	}
}

func (value Uint128) rsh(shift uint) Uint128 {
	if shift >= 64 {
		return Uint128{Lo: value.Hi >> (shift - 64)}
	}
	return Uint128{
		Hi: value.Hi >> shift,
		Lo: value.Lo>>shift | value.Hi<<(64-shift),
	}
}

func (value Uint128) and(other Uint128) Uint128 {
	return Uint128{Hi: value.Hi & other.Hi, Lo: value.Lo & other.Lo}
}

func (value Uint128) or(other Uint128) Uint128 {
	return Uint128{Hi: value.Hi | other.Hi, Lo: value.Lo | other.Lo}
}

func (value Uint128) xor(other Uint128) Uint128 {
	return Uint128{Hi: value.Hi ^ other.Hi, Lo: value.Lo ^ other.Lo}
}

func lowMask(width uint) Uint128 {
	return (Uint128{Hi: ^uint64(0), Lo: ^uint64(0)}).rsh(128 - width)
}

func (value Uint128) andNot(other Uint128) Uint128 {
	return Uint128{Hi: value.Hi &^ other.Hi, Lo: value.Lo &^ other.Lo}
}
