// SPDX-License-Identifier: BSD-2-Clause

//! Deterministic seed-one inputs shared by the continuous and comparison suites.

use std::cmp::Ordering;
use tessera_sort::RadixKey;

/// A key type with a name for case IDs and an exact equality for validation.
pub trait Key: RadixKey + Copy {
    /// Type component of a benchmark case ID.
    const NAME: &'static str;
    /// Draws from the type's whole bit range.
    fn draw(rng: &mut SplitMix64) -> Self;
    /// Draws bounded finite floats; integers use their ordinary random draw.
    fn draw_finite(rng: &mut SplitMix64) -> Self {
        Self::draw(rng)
    }
    /// Draws a key above every `small` value that differs from them in the
    /// highest varying bit.
    fn draw_high(rng: &mut SplitMix64) -> Self;
    /// Converts a value below `2^32` exactly.
    fn small(value: u64) -> Self;
    /// The `k`-th of ten values spread over the whole key, `k` in `0..10`.
    fn tenth(k: u64) -> Self;
    /// Compares keys in the shared total order.
    fn order(&self, other: &Self) -> Ordering;
    /// Tests exact equality, including floating-point payload bits.
    fn same(&self, other: &Self) -> bool;
}

/// Implements exact-width integer key generation and comparison.
macro_rules! int_key {
    ($t:ty, $draw:expr) => {
        impl Key for $t {
            const NAME: &'static str = stringify!($t);
            fn draw(rng: &mut SplitMix64) -> Self {
                $draw(rng)
            }
            fn draw_high(rng: &mut SplitMix64) -> Self {
                Self::draw(rng) | 1 << (<$t>::BITS - 1)
            }
            fn small(value: u64) -> Self {
                value as $t
            }
            fn tenth(k: u64) -> Self {
                <$t>::MAX / 10 * k as $t
            }
            fn order(&self, other: &Self) -> Ordering {
                self.cmp(other)
            }
            fn same(&self, other: &Self) -> bool {
                self == other
            }
        }
    };
}

int_key!(u32, |rng: &mut SplitMix64| rng.next_u64() as u32);
int_key!(u64, |rng: &mut SplitMix64| rng.next_u64());
int_key!(u128, |rng: &mut SplitMix64| {
    (u128::from(rng.next_u64()) << 64) | u128::from(rng.next_u64())
});

impl Key for f64 {
    const NAME: &'static str = "f64";
    // Arbitrary bit patterns include NaNs, infinities and subnormals.
    fn draw(rng: &mut SplitMix64) -> Self {
        f64::from_bits(rng.next_u64())
    }
    // Sign, exponent field 1000..=1029 and random mantissa: magnitudes from
    // 2^-23 to below 2^7, with no NaN or infinity.
    fn draw_finite(rng: &mut SplitMix64) -> Self {
        let sign = rng.next_u64() & (1 << 63);
        let exponent = 1000 + rng.next_u64() % 30;
        f64::from_bits(sign | (exponent << 52) | (rng.next_u64() >> 12))
    }
    // Positive NaNs and +inf have the exponent's top bit set; every `small`
    // value is below 2^32 and has it clear.
    fn draw_high(rng: &mut SplitMix64) -> Self {
        f64::from_bits(0x7ff0_0000_0000_0000 | rng.next_u64() >> 12)
    }
    fn small(value: u64) -> Self {
        value as f64
    }
    // k · 2^1020 is exact and stays finite for k < 16.
    fn tenth(k: u64) -> Self {
        k as f64 * f64::from_bits(0x7fb0_0000_0000_0000)
    }
    fn order(&self, other: &Self) -> Ordering {
        self.total_cmp(other)
    }
    fn same(&self, other: &Self) -> bool {
        self.to_bits() == other.to_bits()
    }
}

/// A row: a key and the record's original index, so validation also checks
/// that records are permuted and that equal keys keep their order.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Row<K> {
    /// Sorting key.
    pub key: K,
    /// Original position in the input.
    pub row: u32,
}

/// A wide C-layout record with the smoke suite's indexed, otherwise zero-filled payload.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, PartialOrd)]
pub struct Wide {
    /// Sorting key.
    pub key: u64,
    /// Observable payload, including the original index in its first eight bytes.
    pub payload: [u8; 120],
}
impl Wide {
    /// Creates exactly the wide input representation used by main tracking.
    pub fn new(key: u64, index: usize) -> Self {
        let mut payload = [0u8; 120];
        payload[..8].copy_from_slice(&(index as u64).to_le_bytes());
        Self { key, payload }
    }
}

/// `SplitMix64` state used to draw reproducible input keys.
pub struct SplitMix64(u64);

impl SplitMix64 {
    /// Advances the generator and returns its next 64 bits.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

/// Generates seed-one inputs with integer arithmetic only.
///
/// Preserves floating-point bit patterns and uses total order for runs.
/// Supported distributions are listed in the benchmark protocol.
///
/// # Panics
///
/// Panics if `distribution` is not a supported name.
///
/// # Examples
///
/// ```
/// use tessera_bench::case::generate;
/// assert_eq!(generate::<u64>("reverse", 3), [2, 1, 0]);
/// ```
pub fn generate<K: Key>(distribution: &str, len: usize) -> Vec<K> {
    let mut rng = SplitMix64(1);
    let n = len as u64;
    match distribution {
        "random" => (0..len).map(|_| K::draw(&mut rng)).collect(),
        "finite_signed" => (0..len).map(|_| K::draw_finite(&mut rng)).collect(),
        "d16" => (0..len).map(|_| K::small(rng.next_u64() % 16)).collect(),
        "d256" => (0..len).map(|_| K::small(rng.next_u64() % 256)).collect(),
        "d4096" => (0..len).map(|_| K::small(rng.next_u64() % 4096)).collect(),
        "zipf1.1" => {
            let mut total = 0u64;
            let cdf: Vec<u64> = (1..=n)
                .map(|k| {
                    total += (1u64 << 48) / (k * tenth_root(k));
                    total
                })
                .collect();
            (0..len)
                .map(|_| {
                    let r = ((u128::from(rng.next_u64()) * u128::from(total)) >> 64) as u64;
                    K::small(cdf.partition_point(|&c| c <= r) as u64)
                })
                .collect()
        }
        "zipf2" => {
            // Zipf with s = 2 over ranks 0..len: rank k has weight
            // floor(2^62 / (k + 1)^2). Rank 0 holds about 61% of the keys.
            let mut total = 0;
            let cdf: Vec<u64> = (1..=n)
                .map(|k| {
                    total += (1 << 62) / (k * k);
                    total
                })
                .collect();
            (0..len)
                .map(|_| {
                    let r = ((u128::from(rng.next_u64()) * u128::from(total)) >> 64) as u64;
                    K::small(cdf.partition_point(|&c| c <= r) as u64)
                })
                .collect()
        }
        "skew99" => {
            // Ten values spread over the whole key; the smallest takes 99% of
            // the records and the other nine share the rest. The spread keeps
            // the minority apart in the top digit.
            (0..len)
                .map(|_| {
                    if rng.next_u64() % 100 < 99 {
                        K::tenth(0)
                    } else {
                        K::tenth(1 + rng.next_u64() % 9)
                    }
                })
                .collect()
        }
        "hot50" => {
            // Half the records take one value and the rest distinct keys above
            // it in the top bit, so the equality partition and a wide child
            // share the node.
            (0..len)
                .map(|_| {
                    if rng.next_u64() >> 63 == 0 {
                        K::small(0)
                    } else {
                        K::draw_high(&mut rng)
                    }
                })
                .collect()
        }
        "two_values" => (0..len)
            .map(|_| K::tenth(if rng.next_u64() >> 63 == 0 { 0 } else { 9 }))
            .collect(),
        "sparse8" => {
            // Each record picks one of eight full-width keys at random, so
            // the sample of runs is not periodic.
            let values: Vec<K> = (0..8).map(|_| K::draw(&mut rng)).collect();
            (0..len)
                .map(|_| values[(rng.next_u64() % 8) as usize])
                .collect()
        }
        "sparse_digits" => {
            // Four random bits per byte: each 8-bit digit takes 16 of its 256
            // values, so a sampled split window never fills its buckets and the
            // engine widens the digit.
            (0..len)
                .map(|_| K::small(rng.next_u64() & 0x5555_5555_5555_5555))
                .collect()
        }
        "bimodal" => {
            // Two clusters 2^63 apart, each uniform over 2^40 keys: the top
            // window holds two values and the node splits into two wide
            // children.
            (0..len)
                .map(|_| {
                    let cluster = (rng.next_u64() & 1) << 63;
                    K::small(cluster | (rng.next_u64() & ((1 << 40) - 1)))
                })
                .collect()
        }
        "sorted_tail" => {
            // Ascending, then the last len / 128 records drawn above them.
            let tail = len / 128;
            let mut values: Vec<K> = (0..(n - tail as u64)).map(K::small).collect();
            values.extend((0..tail).map(|_| K::draw_high(&mut rng)));
            values
        }
        "runs" => {
            // Consecutive runs of 16 to 2,063 random keys, each sorted
            // ascending or descending with equal odds.
            let mut values: Vec<K> = (0..len).map(|_| K::draw(&mut rng)).collect();
            let mut start = 0;
            while start < len {
                let end = len.min(start + 16 + (rng.next_u64() % 2048) as usize);
                let run = &mut values[start..end];
                run.sort_by(K::order);
                if rng.next_u64() >> 63 == 1 {
                    run.reverse();
                }
                start = end;
            }
            values
        }
        "sorted" => (0..n).map(K::small).collect(),
        "reverse" => (0..n).rev().map(K::small).collect(),
        "far_swaps" => {
            // Ascending, with len / 100 swaps between opposite end quarters:
            // each swap spans at least half the array.
            let mut values: Vec<u64> = (0..n).collect();
            let swaps = if len > 1 { (len / 100).max(1) } else { 0 };
            let quarter = (n / 4).max(1);
            for _ in 0..swaps {
                let a = (rng.next_u64() % quarter) as usize;
                let b = (n - 1 - rng.next_u64() % quarter) as usize;
                values.swap(a, b);
            }
            values.into_iter().map(K::small).collect()
        }
        "sorted99" => {
            // Ascending, with len / 100 evenly spaced adjacent swaps.
            let mut values: Vec<u64> = (0..n).collect();
            let step = (len / (len / 100).max(1)).max(2);
            for i in (0..len.saturating_sub(1)).step_by(step) {
                values.swap(i, i + 1);
            }
            values.into_iter().map(K::small).collect()
        }
        "equal" => vec![K::small(1); len],
        _ => unreachable!("unknown distribution {distribution}"),
    }
}

/// Returns the fixed-point tenth root, rounded down in units of 1/256.
fn tenth_root(k: u64) -> u64 {
    let target = u128::from(k) << 80;
    let mut lo = 256u64;
    let mut hi = 2048u64;
    while lo + 1 < hi {
        let mid = (lo + hi) / 2;
        if u128::from(mid).pow(10) <= target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Hashes the canonical little-endian input representation.
    fn hash(bytes: impl Iterator<Item = u8>) -> u64 {
        bytes.fold(0xcbf29ce484222325u64, |a, b| {
            (a ^ u64::from(b)).wrapping_mul(0x100000001b3)
        })
    }
    #[test]
    fn smoke_input_identity() {
        // Pin both lengths and all four key representations with one digest
        // of their canonical hashes per input, rather than four parallel tables.
        let pins = [
            ("random", [0xfae2bab137758ce4, 0xd124d0b0b47c4bbb]),
            ("finite_signed", [0x1dc0714c9161ce23, 0xa5e75273fe9d77fb]),
            ("d16", [0xeeedb599db424e0c, 0x689fc3f3cc1fdd9e]),
            ("d4096", [0xec9ed84b8bf21ec1, 0x051fbab4d83079c9]),
            ("zipf2", [0x1656a17ef6a1927f, 0x29d60cf987a88dad]),
            ("skew99", [0xaad0681ba129d310, 0x49de4a74159baad7]),
            ("hot50", [0x2b2d2e6113113927, 0x4ae1c1d2a89025fb]),
            ("two_values", [0xee18ce4c3edd238e, 0x6682271a50050688]),
            ("sparse8", [0xcab500b74098d84f, 0x8ed4db5e6159508f]),
            ("sparse_digits", [0x1f91201da32c34fd, 0xb8c5581c7f648838]),
            ("bimodal", [0x2a53aadcecc3ad7f, 0x874ed5918561b1b5]),
            ("sorted_tail", [0x085d570ac2c7d1f8, 0x49d7594e21657e7b]),
            ("runs", [0xbf958b207124101d, 0xe14e33b2d463f08e]),
            ("sorted", [0x085d570ac2c7d1f8, 0x3b3cf5f92a0e05a9]),
            ("reverse", [0x70166dae4e23636a, 0xa96a8c2732010185]),
            ("sorted99", [0xcd00fbac35f3b402, 0x9a342e9c307636ac]),
            ("far_swaps", [0xa6f1af7f1c7377c1, 0x45929e88c3f68195]),
            ("equal", [0x9511ccb52fbcbbc5, 0x3566978fb7171531]),
        ];
        for (distribution, expected) in pins {
            for (len, expected) in [127, 4097].into_iter().zip(expected) {
                let actual = [
                    hash(
                        generate::<u32>(distribution, len)
                            .into_iter()
                            .flat_map(u32::to_le_bytes),
                    ),
                    hash(
                        generate::<u64>(distribution, len)
                            .into_iter()
                            .flat_map(u64::to_le_bytes),
                    ),
                    hash(
                        generate::<u128>(distribution, len)
                            .into_iter()
                            .flat_map(u128::to_le_bytes),
                    ),
                    hash(
                        generate::<f64>(distribution, len)
                            .into_iter()
                            .flat_map(|x| x.to_bits().to_le_bytes()),
                    ),
                ];
                assert_eq!(
                    hash(actual.into_iter().flat_map(u64::to_le_bytes)),
                    expected,
                    "{distribution}/{len}"
                );
            }
        }
    }
}
