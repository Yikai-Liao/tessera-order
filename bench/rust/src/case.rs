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
        "d16" => (0..len).map(|_| K::small(rng.next_u64() % 16)).collect(),
        "d256" => (0..len).map(|_| K::small(rng.next_u64() % 256)).collect(),
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

// Fixed-point tenth root, rounded down in units of 1/256.
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
    fn hash(bytes: impl Iterator<Item = u8>) -> u64 {
        bytes.fold(0xcbf29ce484222325u64, |a, b| {
            (a ^ u64::from(b)).wrapping_mul(0x100000001b3)
        })
    }
    #[test]
    fn smoke_inputs_keep_their_published_identity() {
        let pins = [
            (
                "random",
                127,
                [
                    0x7f6817a2413fea1a,
                    0xcbcadab83f300c9d,
                    0xbced6f9334e69fc3,
                    0xcbcadab83f300c9d,
                ],
            ),
            (
                "random",
                4097,
                [
                    0x4bc287e322253b02,
                    0x5ab7b7e4ec8d3771,
                    0x7530fb09539b7747,
                    0x5ab7b7e4ec8d3771,
                ],
            ),
            (
                "d16",
                127,
                [
                    0x8bc7c35b703a0131,
                    0x60dcf31766c85e41,
                    0x111b9f88a44a5561,
                    0x1a114d4fde5797d0,
                ],
            ),
            (
                "d16",
                4097,
                [
                    0x0d53ba0be4e27e7d,
                    0x78c19323a8ed9ecd,
                    0xc6ba7e2d5bd5ee6d,
                    0x8127ffe5c5a20fa2,
                ],
            ),
            (
                "zipf2",
                127,
                [
                    0x7ee841fe204cd02f,
                    0x269690da0f3f4b2f,
                    0x5d74c129248b6a6f,
                    0xaafc7b01ff662d11,
                ],
            ),
            (
                "zipf2",
                4097,
                [
                    0xf76adca29b349851,
                    0xa716834913489cb1,
                    0x6bf9ef76cdda3ab1,
                    0xaa69800e192a523d,
                ],
            ),
            (
                "skew99",
                127,
                [
                    0x0319015562663f55,
                    0x28a6b7476058f085,
                    0x2e2d39fb5bc3d1e5,
                    0x28a6b7476058f085,
                ],
            ),
            (
                "skew99",
                4097,
                [
                    0x43fd928e53fa2511,
                    0xdc89d542eb17eaed,
                    0x9a16038089101215,
                    0x5b2159d324302a3d,
                ],
            ),
            (
                "two_values",
                127,
                [
                    0x1a1012df42610a52,
                    0x3f450ea180587c82,
                    0x18a0be3dbf73e462,
                    0xc2f9ed80fd2d9f8a,
                ],
            ),
            (
                "two_values",
                4097,
                [
                    0x6c6ea427cd3e4215,
                    0xb6524b1ecf43c945,
                    0xade4124e55f57e65,
                    0x0623bec8a966e03d,
                ],
            ),
            (
                "sparse8",
                127,
                [
                    0x248217bf732ba8f9,
                    0xe166b56bb6e972e5,
                    0xb62f9bfeb9d3f44b,
                    0xe166b56bb6e972e5,
                ],
            ),
            (
                "sparse8",
                4097,
                [
                    0xe9234f0d236efa45,
                    0xf9dcd6d3cc34b40b,
                    0x8882bc00c04d3d86,
                    0xf9dcd6d3cc34b40b,
                ],
            ),
            (
                "sorted_tail",
                127,
                [
                    0x7a0ef259907f2b2a,
                    0xf13304768e75bcfa,
                    0x95280bf4f145799a,
                    0xb7ad1943be70f7c7,
                ],
            ),
            (
                "sorted_tail",
                4097,
                [
                    0x923ffcc1351373ce,
                    0x5c5e3e856c34dff7,
                    0x017a0b0ed900e3f7,
                    0xf367cf6329b43a58,
                ],
            ),
            (
                "runs",
                127,
                [
                    0xc773203952b15086,
                    0x254bca296b207835,
                    0xa51f7a281959dab3,
                    0x0dc0edfe6dcd8795,
                ],
            ),
            (
                "runs",
                4097,
                [
                    0x1cad61acb8886886,
                    0x496b83eec2d9c395,
                    0x2a6e309c19526003,
                    0x1f34a8c43e2b507d,
                ],
            ),
            (
                "sorted",
                127,
                [
                    0x7a0ef259907f2b2a,
                    0xf13304768e75bcfa,
                    0x95280bf4f145799a,
                    0xb7ad1943be70f7c7,
                ],
            ),
            (
                "sorted",
                4097,
                [
                    0x14ac3b6d2b9e9945,
                    0x7a574ec056c9b415,
                    0x31998a6e58600ab5,
                    0x4b922f92a82d9060,
                ],
            ),
            (
                "reverse",
                127,
                [
                    0xcfca5a7ed0b7812a,
                    0x043bada1fdfde4fa,
                    0x013b6939a618d99a,
                    0x24b3797977274ea3,
                ],
            ),
            (
                "reverse",
                4097,
                [
                    0x9f2f36182c0f1b45,
                    0xe63a08b87acccc15,
                    0x1e00dc83a7c388b5,
                    0x61ef5b902a0c9bf0,
                ],
            ),
            (
                "sorted99",
                127,
                [
                    0x6ed578a8cfb99aba,
                    0xc07d9bedcad5861a,
                    0x3b584c50c1b471da,
                    0x410a81d67f6b4027,
                ],
            ),
            (
                "sorted99",
                4097,
                [
                    0x8a582303250841e5,
                    0xb6ed6074b4bbd605,
                    0xda3faf098d294745,
                    0xbcc72fd635838090,
                ],
            ),
            (
                "equal",
                127,
                [
                    0x8d4f42e3bcdd5b54,
                    0x0e046911cf776884,
                    0x30f747e34d01c1e4,
                    0x895d76d6f5d74798,
                ],
            ),
            (
                "equal",
                4097,
                [
                    0x35c610ce9520d764,
                    0xcc9834c872c3efa4,
                    0xf398983b5b3c4c24,
                    0xe6b368c0c14b1db8,
                ],
            ),
        ];
        for (distribution, len, expected) in pins {
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
            assert_eq!(actual, expected, "{distribution}/{len}");
        }
    }
}
