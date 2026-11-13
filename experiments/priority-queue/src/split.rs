// SPDX-License-Identifier: BSD-2-Clause
/// One partition decision consumed by the scatter. Equality splitting also
/// consumes the highest varying bit, so a hot-key hint never causes repeated
/// peeling without bit progress on the other ranges.
pub enum Split {
    Digit { shift: u32, mask: usize },
    Equal { key: u64, top: u32 },
}
impl Split {
    pub fn count(&self) -> usize {
        match self {
            Self::Digit { mask, .. } => mask + 1,
            Self::Equal { .. } => 4,
        }
    }
    pub fn bucket(&self, key: u64) -> usize {
        match *self {
            Self::Digit { shift, mask } => ((key >> shift) as usize) & mask,
            Self::Equal { key: hot, top } => {
                let rank = if key < hot {
                    0
                } else if key == hot {
                    1
                } else {
                    2
                };
                rank + ((key >> top) & 1) as usize
            }
        }
    }
}
pub fn choose(
    len: usize,
    min: u64,
    max: u64,
    tile: usize,
    mut get: impl FnMut(usize) -> u64,
) -> Split {
    let span = 64 - (min ^ max).leading_zeros();
    if len >= 4 * tile {
        let mut sample = [0u64; 128];
        for (i, key) in sample.iter_mut().enumerate() {
            let start = i * len / 128;
            let end = (i + 1) * len / 128;
            let width = end - start;
            // Stratified samples with a different golden-ratio position per stratum.
            let phase = ((i as u64 + 1).wrapping_mul(0x9e3779b97f4a7c15)) as u128;
            let offset = ((phase * width as u128) >> 64) as usize;
            *key = get(start + offset);
        }
        sample.sort_unstable();
        let mut start = 0;
        while start < sample.len() {
            let mut end = start + 1;
            while end < sample.len() && sample[end] == sample[start] {
                end += 1;
            }
            if end - start >= sample.len() / 4 {
                return Split::Equal {
                    key: sample[start],
                    top: span - 1,
                };
            }
            start = end;
        }
    }
    let children = len.div_ceil(tile / 2);
    let width = (usize::BITS - (children - 1).leading_zeros())
        .clamp(3, 8)
        .min(span);
    Split::Digit {
        shift: span - width,
        mask: (1usize << width) - 1,
    }
}
