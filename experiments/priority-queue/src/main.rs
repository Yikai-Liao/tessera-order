// SPDX-License-Identifier: BSD-2-Clause
mod alloc;
mod buffered_runs;
mod eager;
mod insert_buffer;
mod lazy_runs;
mod old;
mod range;
mod range_noeq;
mod range_std;
mod runs;
mod split;
use std::{cmp::Reverse, collections::BinaryHeap, sync::atomic::Ordering, time::Instant};
trait Queue {
    fn push(&mut self, key: u64);
    fn pop(&mut self) -> Option<u64>;
}
#[derive(Default)]
struct Heap(BinaryHeap<Reverse<u64>>);
impl Queue for Heap {
    fn push(&mut self, key: u64) {
        self.0.push(Reverse(key));
    }
    fn pop(&mut self) -> Option<u64> {
        self.0.pop().map(|r| r.0)
    }
}
struct Dary<const D: usize>(dary_heap::DaryHeap<Reverse<u64>, D>);
impl<const D: usize> Queue for Dary<D> {
    fn push(&mut self, key: u64) {
        self.0.push(Reverse(key));
    }
    fn pop(&mut self) -> Option<u64> {
        self.0.pop().map(|r| r.0)
    }
}
const LABELS: [&str; 23] = [
    "heap",
    "heap_reserved",
    "radix_old",
    "runs_old",
    "range512",
    "range2048",
    "runs_tree512",
    "runs_lsm512",
    "runs_tree2048",
    "runs_lsm2048",
    "heap_aa",
    "dary4",
    "dary8",
    "eager4",
    "eager8",
    "range_noeq",
    "range8192",
    "range_std",
    "lazy2048",
    "lazy8192",
    "lazy_lsm",
    "adaptive2048",
    "adaptive8192",
];
fn make(label: &str, max_len: usize) -> Box<dyn Queue> {
    match label {
        "heap" | "heap_aa" => Box::<Heap>::default(),
        "heap_reserved" => Box::new(Heap(BinaryHeap::with_capacity(max_len))),
        "radix_old" => Box::<old::Radix>::default(),
        "runs_old" => Box::<old::Runs>::default(),
        "range512" => Box::<range::RangeQueue<512>>::default(),
        "range2048" => Box::<range::RangeQueue<2048>>::default(),
        "range_noeq" => Box::<range_noeq::RangeQueue<2048>>::default(),
        "range8192" => Box::<range::RangeQueue<8192>>::default(),
        "range_std" => Box::<range_std::RangeQueue<8192>>::default(),
        "lazy2048" => Box::<lazy_runs::LazyRunQueue<2048, false>>::default(),
        "lazy8192" => Box::<lazy_runs::LazyRunQueue<8192, false>>::default(),
        "lazy_lsm" => Box::<lazy_runs::LazyRunQueue<8192, true>>::default(),
        "adaptive2048" => Box::<buffered_runs::BufferedRunQueue<2048, false>>::default(),
        "adaptive8192" => Box::<buffered_runs::BufferedRunQueue<8192, false>>::default(),
        "runs_tree512" => Box::<runs::RunQueue<512, false>>::default(),
        "runs_lsm512" => Box::<runs::RunQueue<512, true>>::default(),
        "runs_tree2048" => Box::<runs::RunQueue<2048, false>>::default(),
        "runs_lsm2048" => Box::<runs::RunQueue<2048, true>>::default(),
        "dary4" => Box::new(Dary::<4>(dary_heap::DaryHeap::with_capacity(max_len))),
        "dary8" => Box::new(Dary::<8>(dary_heap::DaryHeap::with_capacity(max_len))),
        "eager4" => Box::<eager::EagerQueue<4, 2048>>::default(),
        "eager8" => Box::<eager::EagerQueue<8, 2048>>::default(),
        _ => panic!("unknown queue {label}"),
    }
}
#[derive(Clone, Copy)]
enum Op {
    Push(u64),
    Pop,
}
fn rand(x: &mut u64) -> u64 {
    *x ^= *x << 13;
    *x ^= *x >> 7;
    *x ^= *x << 17;
    *x
}
const CASES: [&str; 18] = [
    "drain",
    "top1",
    "mixed",
    "newmin",
    "equal",
    "survivor",
    "fragment",
    "d1024",
    "skew90",
    "ascending",
    "descending",
    "steady",
    "bursts",
    "shortcycles",
    "randomwalk",
    "monotone",
    "frontmix",
    "sparsebands",
];
fn trace(name: &str, n: usize, seed: u64) -> Vec<Op> {
    let mut s = seed;
    let mut a = Vec::new();
    match name {
        "drain" | "top1" | "equal" | "d1024" | "skew90" | "ascending" | "descending" => {
            for i in 0..n {
                let key = match name {
                    "equal" => 7,
                    "d1024" => rand(&mut s) % 1024,
                    "skew90" => {
                        if !rand(&mut s).is_multiple_of(10) {
                            7
                        } else {
                            rand(&mut s)
                        }
                    }
                    "ascending" => i as u64,
                    "descending" => (n - i) as u64,
                    _ => rand(&mut s),
                };
                a.push(Op::Push(key));
            }
            for _ in 0..if name == "top1" { n / 100 } else { n } {
                a.push(Op::Pop);
            }
        }
        "mixed" | "steady" => {
            for _ in 0..n {
                a.push(Op::Push(rand(&mut s)));
            }
            for _ in 0..n * if name == "steady" { 4 } else { 1 } {
                a.push(Op::Pop);
                a.push(Op::Push(rand(&mut s)));
            }
        }
        "newmin" => {
            for _ in 0..n {
                a.push(Op::Push((1u64 << 63) | rand(&mut s) >> 1));
            }
            for i in 0..n {
                a.push(Op::Push((n - i) as u64));
                a.push(Op::Pop);
            }
        }
        "fragment" => {
            for _ in 0..n {
                a.push(Op::Push(rand(&mut s)));
            }
            for _ in 0..n {
                a.push(Op::Pop);
            }
            for _ in 0..n / 512 {
                a.push(Op::Push(rand(&mut s)));
            }
        }
        "survivor" => {
            for round in 0..n / 256 {
                for _ in 0..255 {
                    a.push(Op::Push(rand(&mut s) & 0xffff));
                }
                a.push(Op::Push((1u64 << 63) | rand(&mut s)));
                for _ in 0..255 {
                    a.push(Op::Pop);
                }
                if round % 16 == 0 {
                    a.push(Op::Push((1u64 << 63) | rand(&mut s)));
                }
            }
        }
        "bursts" => {
            for _ in 0..n {
                a.push(Op::Push(rand(&mut s)));
            }
            for _ in 0..n / 256 {
                for _ in 0..256 {
                    a.push(Op::Pop);
                }
                for _ in 0..256 {
                    a.push(Op::Push(rand(&mut s)));
                }
            }
        }
        "shortcycles" => {
            for _ in 0..n / 256 {
                for _ in 0..256 {
                    a.push(Op::Push(rand(&mut s)));
                }
                for _ in 0..256 {
                    a.push(Op::Pop);
                }
            }
        }
        "randomwalk" => {
            for _ in 0..n {
                a.push(Op::Push(rand(&mut s)));
            }
            for _ in 0..4 * n {
                if rand(&mut s) & 1 == 0 {
                    a.push(Op::Push(rand(&mut s)));
                } else {
                    a.push(Op::Pop);
                }
            }
        }
        "frontmix" => {
            for _ in 0..n {
                a.push(Op::Push((1u64 << 63) | rand(&mut s)));
            }
            a.push(Op::Pop);
            for _ in 0..n / 256 {
                for _ in 0..256 {
                    a.push(Op::Push(rand(&mut s) & 0xffff));
                }
                for _ in 0..256 {
                    a.push(Op::Pop);
                }
            }
        }
        "sparsebands" => {
            for _ in 0..n / 256 {
                for _ in 0..255 {
                    a.push(Op::Push(rand(&mut s) & 0xffff));
                }
                a.push(Op::Push((1u64 << 63) | rand(&mut s)));
                a.push(Op::Pop);
                for _ in 0..254 {
                    a.push(Op::Pop);
                }
            }
            for _ in 0..n / 8 {
                a.push(Op::Push(rand(&mut s)));
                a.push(Op::Pop);
            }
        }
        "monotone" => {
            let mut reference = Heap::default();
            for _ in 0..n {
                let key = rand(&mut s) % (4 * n as u64 + 1);
                reference.push(key);
                a.push(Op::Push(key));
            }
            for _ in 0..4 * n {
                let key = reference.pop().unwrap();
                a.push(Op::Pop);
                let next = key + rand(&mut s) % (4 * n as u64 + 1);
                reference.push(next);
                a.push(Op::Push(next));
            }
        }
        _ => panic!("unknown trace {name}"),
    };
    a
}
fn execute(q: &mut dyn Queue, ops: &[Op], out: &mut Vec<Option<u64>>) {
    for op in ops {
        match *op {
            Op::Push(k) => q.push(k),
            Op::Pop => out.push(q.pop()),
        }
    }
}
fn quantile(times: &[u128], pct: usize) -> u128 {
    times
        .get(times.len().saturating_sub(1) * pct / 100)
        .copied()
        .unwrap_or(0)
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let n: usize = args.get(1).map_or(4096, |s| s.parse().unwrap());
    let reps: usize = args.get(2).map_or(3, |s| s.parse().unwrap());
    let mode = args.get(3).map_or("time", String::as_str);
    let labels: Vec<_> = args
        .get(4)
        .map_or_else(|| LABELS.to_vec(), |s| s.split(',').collect());
    let cases: Vec<_> = args
        .get(5)
        .map_or_else(|| CASES.to_vec(), |s| s.split(',').collect());
    assert!(mode == "latency" || mode == "time");
    println!(
        "case,n,seed,round,algorithm,ops,total_ns,load_ns,online_ns,peak_bytes,live_bytes,pop_p50_ns,pop_p99_ns,pop_max_ns,first_pop_ns,push_p99_ns,push_max_ns"
    );
    for name in cases {
        for seed in [
            1,
            0x123456789abcdefu64,
            0x3141592653589793,
            0xd1b54a32d192ed03,
        ] {
            let ops = trace(name, n, seed);
            let pops = ops.iter().filter(|o| matches!(o, Op::Pop)).count();
            let first_pop = ops
                .iter()
                .position(|o| matches!(o, Op::Pop))
                .unwrap_or(ops.len());
            let (mut len, mut max_len) = (0usize, 0usize);
            for op in &ops {
                match op {
                    Op::Push(_) => {
                        len += 1;
                        max_len = max_len.max(len);
                    }
                    Op::Pop => {
                        len = len.saturating_sub(1);
                    }
                }
            }
            let mut expected = Vec::with_capacity(pops);
            let mut reference = Heap::default();
            execute(&mut reference, &ops, &mut expected);
            let mut remaining = Vec::with_capacity(len);
            while let Some(key) = reference.pop() {
                remaining.push(key);
            }
            for round in 0..reps {
                for j in 0..labels.len() {
                    let label = labels[(j + round) % labels.len()];
                    let mut out = Vec::with_capacity(pops);
                    let mut pop_times =
                        Vec::with_capacity(if mode == "latency" { pops } else { 0 });
                    let mut push_times = Vec::with_capacity(if mode == "latency" {
                        ops.len() - pops
                    } else {
                        0
                    });
                    let base = alloc::LIVE.load(Ordering::Relaxed);
                    alloc::PEAK.store(base, Ordering::Relaxed);
                    let start = Instant::now();
                    let mut q = make(label, max_len);
                    let (load_ns, online_ns, first_pop_ns);
                    if mode == "latency" {
                        for op in &ops {
                            let t = Instant::now();
                            match *op {
                                Op::Push(k) => {
                                    q.push(k);
                                    push_times.push(t.elapsed().as_nanos());
                                }
                                Op::Pop => {
                                    let key = q.pop();
                                    pop_times.push(t.elapsed().as_nanos());
                                    out.push(key);
                                }
                            }
                        }
                        load_ns = 0;
                        online_ns = 0;
                        first_pop_ns = pop_times.first().copied().unwrap_or(0);
                    } else {
                        execute(q.as_mut(), &ops[..first_pop], &mut out);
                        load_ns = start.elapsed().as_nanos();
                        let t = Instant::now();
                        execute(q.as_mut(), &ops[first_pop..], &mut out);
                        online_ns = t.elapsed().as_nanos();
                        first_pop_ns = 0;
                    }
                    let total_ns = start.elapsed().as_nanos();
                    let peak = alloc::PEAK.load(Ordering::Relaxed).saturating_sub(base);
                    let live = alloc::LIVE.load(Ordering::Relaxed).saturating_sub(base);
                    assert_eq!(
                        out, expected,
                        "trace {name}, {label}, n={n}, seed={seed}, round={round}"
                    );
                    // Validate even the records not requested by this trace; their
                    // eventual drain is outside timing and allocation snapshots.
                    for &key in &remaining {
                        assert_eq!(q.pop(), Some(key), "remaining {name} {label}");
                    }
                    assert_eq!(q.pop(), None, "extra record {name} {label}");
                    drop(q);
                    #[cfg(feature = "memory")]
                    assert_eq!(
                        alloc::LIVE.load(Ordering::Relaxed),
                        base,
                        "queue allocation leak"
                    );
                    pop_times.sort_unstable();
                    push_times.sort_unstable();
                    println!(
                        "{name},{n},{seed},{round},{label},{},{total_ns},{load_ns},{online_ns},{peak},{live},{},{},{},{first_pop_ns},{},{}",
                        ops.len(),
                        quantile(&pop_times, 50),
                        quantile(&pop_times, 99),
                        pop_times.last().copied().unwrap_or(0),
                        quantile(&push_times, 99),
                        push_times.last().copied().unwrap_or(0)
                    );
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn candidates() -> Vec<Box<dyn Queue>> {
        LABELS[4..10]
            .iter()
            .chain(LABELS[11..].iter())
            .map(|s| make(s, 0))
            .collect()
    }
    #[test]
    fn random_dynamic_traces() {
        for seed in 1..81 {
            let mut s = seed;
            let mut qs = candidates();
            let mut reference = Heap::default();
            for i in 0..10000 {
                if rand(&mut s) % 5 < 3 {
                    let key = match i % 9 {
                        0 => 0,
                        1 => u64::MAX,
                        2 => 7,
                        3 => rand(&mut s) % 1024,
                        _ => rand(&mut s),
                    };
                    reference.push(key);
                    for q in &mut qs {
                        q.push(key);
                    }
                } else {
                    let key = reference.pop();
                    for q in &mut qs {
                        assert_eq!(q.pop(), key, "seed {seed}, op {i}");
                    }
                }
            }
            loop {
                let key = reference.pop();
                for q in &mut qs {
                    assert_eq!(q.pop(), key, "final drain seed {seed}");
                }
                if key.is_none() {
                    break;
                }
            }
        }
    }
    #[test]
    fn operation_matrix() {
        for n in [1, 48, 256, 511, 512, 513, 2047, 2048, 2049, 8192] {
            for case in CASES {
                let ops = trace(case, n, 12345);
                let mut expected = Vec::new();
                execute(&mut Heap::default(), &ops, &mut expected);
                for mut q in candidates() {
                    let mut out = Vec::new();
                    execute(q.as_mut(), &ops, &mut out);
                    assert_eq!(out, expected, "{case}, n={n}");
                }
            }
        }
    }
    #[test]
    fn refill_existing_front_and_reopen_ranges() {
        for mut q in candidates() {
            let mut refq = Heap::default();
            for cycle in 0..12u64 {
                for i in 0..4097 {
                    let key = if cycle % 3 == 0 {
                        u64::MAX
                    } else {
                        (i << 48) ^ cycle
                    };
                    q.push(key);
                    refq.push(key);
                }
                assert_eq!(q.pop(), refq.pop());
                for i in 0..4097 {
                    let key = 4097 - i;
                    q.push(key);
                    refq.push(key);
                }
                for _ in 0..8193 {
                    assert_eq!(q.pop(), refq.pop());
                }
                assert_eq!(q.pop(), None);
            }
        }
    }
}
