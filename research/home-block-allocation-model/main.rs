// SPDX-License-Identifier: BSD-2-Clause
//! Safe allocation-only falsifier. No payload, unsafe code, threads or clocks.
#![forbid(unsafe_code)]

use std::mem::size_of;

const NONE: usize = usize::MAX;
const INCREMENT: u64 = 0x9e3779b97f4a7c15;
const SCHEDULE_SEED: u64 = 0x484f4d455f424c4b;

/// Evaluates exactly the SplitMix64 value at this original input index.
fn splitmix(index: usize, seed: u64) -> u64 {
    let mut value = seed.wrapping_add(INCREMENT.wrapping_mul(index as u64 + 1));
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

#[derive(Clone, Copy)]
enum Pattern {
    SplitMix,
    OneBucket,
}

#[derive(Clone, Copy)]
struct Case {
    name: &'static str,
    records: usize,
    block: usize,
    shards: usize,
    radix: usize,
    shift: usize,
    pattern: Pattern,
}

impl Case {
    fn bucket(self, index: usize) -> usize {
        match self.pattern {
            Pattern::SplitMix => (splitmix(index, 1) >> self.shift) as usize & (self.radix - 1),
            Pattern::OneBucket => 0,
        }
    }
}

/// Immutable baseline geometry. No cached keys or n-record token array exists.
struct Geometry {
    case: Case,
    full: usize,
    logical: usize,
    total: usize,
    boundary: usize,
    ranges: Vec<(usize, usize)>,
    counts: Vec<usize>,
    starts: Vec<usize>,
    boundary_map: Vec<usize>,
}

impl Geometry {
    fn new(mut case: Case) -> Self {
        assert!(case.records > 0 && case.block > 0 && case.radix.is_power_of_two());
        let full = case.records / case.block;
        case.shards = case.shards.max(1).min(case.records).min(full.max(1));
        let logical = case.records.div_ceil(case.block);
        let mut ranges = Vec::with_capacity(case.shards);
        let mut first = 0;
        for shard in 0..case.shards {
            let blocks = full / case.shards + usize::from(shard < full % case.shards);
            let end = if shard + 1 == case.shards {
                case.records
            } else {
                (first + blocks) * case.block
            };
            ranges.push((first * case.block, end));
            first += blocks;
        }
        let mut counts = vec![0; case.shards * case.radix];
        for (shard, &(begin, end)) in ranges.iter().enumerate() {
            for index in begin..end {
                counts[shard * case.radix + case.bucket(index)] += 1;
            }
        }
        let mut starts = vec![0; counts.len()];
        let mut boundary_map = vec![NONE; logical];
        let mut boundary = 0;
        let mut rank = 0;
        for bucket in 0..case.radix {
            for shard in 0..case.shards {
                let segment = shard * case.radix + bucket;
                starts[segment] = rank;
                if rank < case.records && rank % case.block != 0
                    && boundary_map[rank / case.block] == NONE
                {
                    boundary_map[rank / case.block] = full + boundary;
                    boundary += 1;
                }
                rank += counts[segment];
            }
        }
        assert_eq!(rank, case.records);
        if case.records % case.block != 0 && boundary_map[full] == NONE {
            boundary_map[full] = full + boundary;
            boundary += 1;
        }
        assert!(boundary <= case.shards * case.radix);
        Self {
            case, full, logical, total: full + boundary + case.shards * case.radix,
            boundary, ranges, counts, starts, boundary_map,
        }
    }

    fn heap_bytes(&self) -> usize {
        self.ranges.capacity() * size_of::<(usize, usize)>()
            + (self.counts.capacity() + self.starts.capacity() + self.boundary_map.capacity())
                * size_of::<usize>()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    Unread,
    Free { pool: usize, index: usize },
    Occupied { logical: usize },
}

#[derive(Clone, Copy, Debug)]
struct Observation {
    pool: usize,
    index: usize,
}

#[derive(Default)]
struct Operations {
    location_reads: usize,
    location_writes: usize,
    pool_pops: usize,
    pool_pushes: usize,
    pool_swap_removes: usize,
    pool_entry_replacements: usize,
    local_transactions: usize,
    two_pool_transactions: usize,
    lock_equivalent_acquisitions: usize,
    failed_revalidations: usize,
}

/// Exact ownership table plus inverse free-list positions, not an atomic implementation.
struct Pools {
    slots: Vec<Slot>,
    free: Vec<Vec<usize>>,
    retired: Vec<usize>,
    allocated: Vec<usize>,
    seed_credits: usize,
    free_input: usize,
    free_total: usize,
    minimum_pool: usize,
    operations: Operations,
}

impl Pools {
    fn new(geometry: &Geometry) -> Self {
        let c = geometry.case;
        let mut slots = vec![Slot::Unread; geometry.total];
        for (logical, &physical) in geometry.boundary_map.iter().enumerate() {
            if physical != NONE {
                slots[physical] = Slot::Occupied { logical };
            }
        }
        let mut free = Vec::with_capacity(c.shards);
        for (pool, &(begin, end)) in geometry.ranges.iter().enumerate() {
            let mut entries = Vec::with_capacity((end - begin) / c.block + c.radix);
            for index in 0..c.radix {
                let physical = geometry.full + geometry.boundary + pool * c.radix + index;
                entries.push(physical);
                slots[physical] = Slot::Free { pool, index };
            }
            free.push(entries);
        }
        Self {
            slots, free, retired: vec![0; c.shards], allocated: vec![0; c.shards],
            seed_credits: c.radix, free_input: 0, free_total: c.shards * c.radix,
            minimum_pool: c.radix, operations: Operations::default(),
        }
    }

    /// Counts a lock transaction in canonical numeric order, but takes no locks.
    fn transaction(&mut self, requester: usize, foreign: usize) {
        let (first, second) = (requester.min(foreign), requester.max(foreign));
        assert!(first <= second && second < self.free.len());
        if first == second {
            self.operations.local_transactions += 1;
            self.operations.lock_equivalent_acquisitions += 1;
        } else {
            self.operations.two_pool_transactions += 1;
            self.operations.lock_equivalent_acquisitions += 2;
        }
    }

    fn observe(&mut self, desired: usize) -> Option<Observation> {
        self.operations.location_reads += 1;
        match self.slots[desired] {
            Slot::Free { pool, index } => Some(Observation { pool, index }),
            _ => None,
        }
    }

    /// Removes one valid local free entry; credit accounting is committed by allocate.
    fn pop(&mut self, pool: usize) -> usize {
        let index = self.free[pool].len().checked_sub(1).expect("free-pool starvation");
        let physical = self.free[pool].pop().unwrap();
        assert_eq!(self.slots[physical], Slot::Free { pool, index });
        self.operations.pool_pops += 1;
        physical
    }

    /// Revalidates an optional prior observation. A stale result falls back once.
    fn allocate_observed(
        &mut self, requester: usize, desired: usize, observed: Option<Observation>, full: usize,
    ) -> usize {
        assert!(!self.free[requester].is_empty(), "free-pool starvation before allocation");
        let physical = if let Some(location) = observed {
            self.transaction(requester, location.pool);
            self.operations.location_reads += 1;
            if self.slots[desired] == (Slot::Free { pool: location.pool, index: location.index })
                && self.free[location.pool].get(location.index) == Some(&desired)
            {
                if requester == location.pool {
                    assert_eq!(self.free[requester].swap_remove(location.index), desired);
                    self.operations.pool_swap_removes += 1;
                    if location.index < self.free[requester].len() {
                        let moved = self.free[requester][location.index];
                        self.slots[moved] = Slot::Free { pool: requester, index: location.index };
                        self.operations.location_writes += 1;
                        self.operations.pool_entry_replacements += 1;
                    }
                } else {
                    let donor = self.pop(requester);
                    self.free[location.pool][location.index] = donor;
                    self.slots[donor] = Slot::Free { pool: location.pool, index: location.index };
                    self.operations.location_writes += 1;
                    self.operations.pool_entry_replacements += 1;
                }
                desired
            } else {
                self.operations.failed_revalidations += 1;
                self.transaction(requester, requester);
                self.pop(requester)
            }
        } else {
            self.transaction(requester, requester);
            self.pop(requester)
        };
        assert!(matches!(self.slots[physical], Slot::Free { .. }));
        self.slots[physical] = Slot::Occupied { logical: desired };
        self.operations.location_writes += 1;
        self.allocated[requester] += 1;
        self.free_total -= 1;
        self.free_input -= usize::from(physical < full);
        self.minimum_pool = self.minimum_pool.min(self.free[requester].len());
        assert_eq!(self.free[requester].len(),
            self.seed_credits + self.retired[requester] - self.allocated[requester]);
        physical
    }

    /// Publishes an original full source only after all its record writes finish.
    fn release(&mut self, original_owner: usize, physical: usize, full: usize) {
        assert!(physical < full);
        assert_eq!(self.slots[physical], Slot::Unread);
        self.transaction(original_owner, original_owner);
        let index = self.free[original_owner].len();
        assert!(index < self.free[original_owner].capacity());
        self.free[original_owner].push(physical);
        self.slots[physical] = Slot::Free { pool: original_owner, index };
        self.retired[original_owner] += 1;
        self.free_input += 1;
        self.free_total += 1;
        self.operations.location_writes += 1;
        self.operations.pool_pushes += 1;
        assert_eq!(self.free[original_owner].len(),
            self.seed_credits + self.retired[original_owner] - self.allocated[original_owner]);
    }

    /// Bidirectional checks find duplicates and stale inverse entries without an extra array.
    fn validate(&self, full: usize) {
        let mut free_input = 0;
        let mut free_total = 0;
        for (pool, entries) in self.free.iter().enumerate() {
            assert_eq!(entries.len(), self.seed_credits + self.retired[pool] - self.allocated[pool]);
            for (index, &physical) in entries.iter().enumerate() {
                assert_eq!(self.slots[physical], Slot::Free { pool, index });
            }
        }
        for (physical, slot) in self.slots.iter().enumerate() {
            if let Slot::Free { pool, index } = *slot {
                assert_eq!(self.free[pool][index], physical);
                free_total += 1;
                free_input += usize::from(physical < full);
            }
        }
        assert_eq!(self.free_total, free_total);
        assert_eq!(self.free_input, free_input);
    }

    fn heap_bytes(&self) -> usize {
        self.slots.capacity() * size_of::<Slot>()
            + self.free.capacity() * size_of::<Vec<usize>>()
            + self.free.iter().map(|pool| pool.capacity() * size_of::<usize>()).sum::<usize>()
            + (self.retired.capacity() + self.allocated.capacity()) * size_of::<usize>()
    }
}

/// Each cursor owns one contiguous stable-rank segment, including its boundary fragments.
#[derive(Clone)]
struct Cursor {
    next_logical: usize,
    first_offset: usize,
    end_rank: usize,
    rank: usize,
    physical: usize,
    remaining: usize,
}

#[derive(Clone, Copy)]
enum Schedule {
    RoundRobin,
    SerialShards,
    RandomBlocks,
}

impl Schedule {
    fn name(self) -> &'static str {
        match self {
            Self::RoundRobin => "round_robin",
            Self::SerialShards => "serial_whole_shard",
            Self::RandomBlocks => "random_seed_484f4d455f424c4b",
        }
    }
}

#[derive(Default)]
struct AllocationStats {
    bodies: usize,
    boundaries: usize,
    free_same: usize,
    free_foreign: usize,
    unread: usize,
    occupied: usize,
    actual_home: usize,
    inventory_samples: usize,
    free_input_sum: u64,
    free_input_max: usize,
    free_total_max: usize,
    progress_bodies: [usize; 10],
    progress_homes: [usize; 10],
    record_writes: usize,
    source_blocks: usize,
}

#[derive(Default, Clone, PartialEq, Eq, Debug)]
struct Topology {
    paths: usize,
    cycles: usize,
    fixed: usize,
    largest: usize,
    natural_copies: usize,
    fallback: bool,
    directory_hash: u64,
}

/// Counts natural restore copies, including one temporary full copy per nontrivial cycle.
fn topology(directory: &[usize], geometry: &Geometry) -> Topology {
    let mut incoming = vec![false; geometry.total];
    for &physical in directory {
        assert!(physical < geometry.total && !incoming[physical]);
        incoming[physical] = true;
    }
    let mut visited = vec![false; geometry.full];
    let mut report = Topology::default();
    for start in 0..geometry.full {
        if incoming[start] { continue; }
        let mut at = start;
        let mut length = 0;
        while at < geometry.full {
            assert!(!visited[at]);
            visited[at] = true;
            length += 1;
            at = directory[at];
        }
        report.paths += 1;
        report.largest = report.largest.max(length);
    }
    for start in 0..geometry.full {
        if visited[start] { continue; }
        let mut at = start;
        let mut length = 0;
        loop {
            assert!(at < geometry.full && !visited[at]);
            visited[at] = true;
            length += 1;
            at = directory[at];
            if at == start { break; }
        }
        if length == 1 { report.fixed += 1; } else { report.cycles += 1; }
        report.largest = report.largest.max(length);
    }
    report.natural_copies = (geometry.full - report.fixed + report.cycles) * geometry.case.block
        + geometry.case.records % geometry.case.block;
    report.fallback = report.largest > geometry.full.div_ceil(geometry.case.shards).max(1) * 2;
    report.directory_hash = directory.iter().enumerate().fold(0u64, |hash, (logical, &physical)| {
        hash.wrapping_mul(INCREMENT).wrapping_add((logical as u64).rotate_left(17) ^ physical as u64)
    });
    report
}

struct Model<'a> {
    geometry: &'a Geometry,
    pools: Pools,
    directory: Vec<usize>,
    initialized: Vec<usize>,
    cursors: Vec<Cursor>,
    next_source: Vec<usize>,
    stats: AllocationStats,
}

impl<'a> Model<'a> {
    fn new(geometry: &'a Geometry) -> Self {
        let c = geometry.case;
        let cursors = geometry.starts.iter().zip(&geometry.counts).map(|(&start, &count)| Cursor {
            next_logical: start / c.block, first_offset: start % c.block,
            end_rank: start + count, rank: start, physical: NONE, remaining: 0,
        }).collect();
        Self {
            geometry, pools: Pools::new(geometry), directory: geometry.boundary_map.clone(),
            initialized: vec![0; geometry.logical], cursors,
            next_source: geometry.ranges.iter().map(|&(begin, _)| begin).collect(),
            stats: AllocationStats::default(),
        }
    }

    /// Allocates only at the first record write; occupied targets are never evicted.
    fn begin_fragment(&mut self, shard: usize, segment: usize, policy: bool) {
        let g = self.geometry;
        let c = g.case;
        let cursor = &self.cursors[segment];
        let logical = cursor.next_logical;
        let base = logical * c.block;
        assert!(base < cursor.end_rank);
        let end_offset = (cursor.end_rank - base).min(c.block);
        assert!(cursor.first_offset < end_offset);
        assert_eq!(cursor.rank, base + cursor.first_offset);
        let physical = if g.boundary_map[logical] != NONE {
            self.stats.boundaries += 1;
            g.boundary_map[logical]
        } else {
            assert_eq!(cursor.first_offset, 0);
            assert_eq!(end_offset, c.block);
            assert!(logical < g.full);
            assert_eq!(self.directory[logical], NONE);
            self.stats.bodies += 1;
            self.stats.inventory_samples += 1;
            self.stats.free_input_sum += self.pools.free_input as u64;
            self.stats.free_input_max = self.stats.free_input_max.max(self.pools.free_input);
            self.stats.free_total_max = self.stats.free_total_max.max(self.pools.free_total);
            match self.pools.slots[logical] {
                Slot::Unread => self.stats.unread += 1,
                Slot::Occupied { .. } => self.stats.occupied += 1,
                Slot::Free { pool, .. } if pool == shard => self.stats.free_same += 1,
                Slot::Free { .. } => self.stats.free_foreign += 1,
            }
            let observed = if policy { self.pools.observe(logical) } else { None };
            let physical = self.pools.allocate_observed(shard, logical, observed, g.full);
            self.directory[logical] = physical;
            let progress = (self.stats.record_writes * 10 / c.records).min(9);
            self.stats.progress_bodies[progress] += 1;
            if physical == logical {
                self.stats.actual_home += 1;
                self.stats.progress_homes[progress] += 1;
            }
            physical
        };
        let cursor = &mut self.cursors[segment];
        cursor.physical = physical;
        cursor.remaining = end_offset - cursor.first_offset;
        cursor.first_offset = 0;
        cursor.next_logical += 1;
    }

    /// Source records stay in order; even record-quantum fixtures retire only after the final write.
    fn step(&mut self, shard: usize, policy: bool, record_quantum: bool) {
        let g = self.geometry;
        let c = g.case;
        let begin = self.next_source[shard];
        let source_end = ((begin / c.block + 1) * c.block).min(g.ranges[shard].1);
        let end = if record_quantum { (begin + 1).min(source_end) } else { source_end };
        for index in begin..end {
            if index / c.block < g.full {
                assert_eq!(self.pools.slots[index / c.block], Slot::Unread);
            }
            let segment = shard * c.radix + c.bucket(index);
            if self.cursors[segment].remaining == 0 {
                self.begin_fragment(shard, segment, policy);
            }
            let cursor = &mut self.cursors[segment];
            assert!(cursor.rank < cursor.end_rank && cursor.remaining > 0);
            let logical = cursor.rank / c.block;
            assert_eq!(self.directory[logical], cursor.physical);
            assert_eq!(self.pools.slots[cursor.physical], Slot::Occupied { logical });
            // Segments partition ranks, and each cursor advances strictly once per input.
            // These counts validate complete initialization without storing record tokens.
            self.initialized[logical] += 1;
            assert!(self.initialized[logical] <= c.block.min(c.records - logical * c.block));
            cursor.rank += 1;
            cursor.remaining -= 1;
            self.stats.record_writes += 1;
        }
        self.next_source[shard] = end;
        if end == source_end && end % c.block == 0 {
            self.pools.release(shard, end / c.block - 1, g.full);
            self.stats.source_blocks += 1;
        }
        self.stats.free_input_max = self.stats.free_input_max.max(self.pools.free_input);
        self.stats.free_total_max = self.stats.free_total_max.max(self.pools.free_total);
        assert!(self.pools.free_total <= c.shards * c.radix + g.boundary);
    }

    fn run(&mut self, policy: bool, schedule: Schedule, record_quantum: bool) {
        self.pools.validate(self.geometry.full);
        let mut turn = 0;
        let mut random_counter = 0;
        let mut quanta = 0;
        while self.stats.record_writes < self.geometry.case.records {
            let shards = self.geometry.case.shards;
            let shard = match schedule {
                Schedule::SerialShards => (0..shards)
                    .find(|&s| self.next_source[s] < self.geometry.ranges[s].1).unwrap(),
                Schedule::RoundRobin => {
                    let next = (0..shards).map(|offset| (turn + offset) % shards)
                        .find(|&s| self.next_source[s] < self.geometry.ranges[s].1).unwrap();
                    turn = (next + 1) % shards;
                    next
                }
                Schedule::RandomBlocks => {
                    let active = (0..shards).filter(|&s| self.next_source[s] < self.geometry.ranges[s].1).count();
                    let selected = splitmix(random_counter, SCHEDULE_SEED) as usize % active;
                    random_counter += 1;
                    (0..shards).filter(|&s| self.next_source[s] < self.geometry.ranges[s].1)
                        .nth(selected).unwrap()
                }
            };
            self.step(shard, policy, record_quantum);
            quanta += 1;
            if record_quantum || quanta % 2048 == 0 {
                self.pools.validate(self.geometry.full);
            }
        }
        self.validate();
    }

    fn validate(&self) {
        let g = self.geometry;
        let c = g.case;
        self.pools.validate(g.full);
        assert_eq!(self.stats.record_writes, c.records);
        assert_eq!(self.stats.source_blocks, g.full);
        assert_eq!(self.stats.bodies + g.boundary, g.logical);
        assert_eq!(self.stats.bodies,
            self.stats.free_same + self.stats.free_foreign + self.stats.unread + self.stats.occupied);
        for cursor in &self.cursors {
            assert_eq!(cursor.rank, cursor.end_rank);
            assert_eq!(cursor.remaining, 0);
            if cursor.physical == NONE {
                assert_eq!(cursor.next_logical * c.block + cursor.first_offset, cursor.end_rank);
            } else {
                assert_eq!(cursor.next_logical, cursor.end_rank.div_ceil(c.block));
            }
        }
        for (logical, &physical) in self.directory.iter().enumerate() {
            assert!(physical < g.total);
            assert_eq!(self.pools.slots[physical], Slot::Occupied { logical });
            assert_eq!(self.initialized[logical], c.block.min(c.records - logical * c.block));
            if g.boundary_map[logical] != NONE {
                assert_eq!(physical, g.boundary_map[logical]);
            }
        }
        for (physical, &slot) in self.pools.slots.iter().enumerate() {
            match slot {
                Slot::Occupied { logical } => assert_eq!(self.directory[logical], physical),
                Slot::Unread => panic!("original source was not fully retired"),
                Slot::Free { .. } => {}
            }
        }
    }

    fn heap_bytes(&self) -> usize {
        self.geometry.heap_bytes() + self.pools.heap_bytes()
            + (self.directory.capacity() + self.initialized.capacity() + self.next_source.capacity())
                * size_of::<usize>()
            + self.cursors.capacity() * size_of::<Cursor>()
    }
}

/// Staged observations exercise protocol races; each committed pool mutation is atomic.
fn transaction_fixtures() {
    let case = Case { name: "transactions", records: 16, block: 4, shards: 2,
        radix: 2, shift: 63, pattern: Pattern::OneBucket };
    let g = Geometry::new(case);
    assert_eq!(g.boundary, 0);
    // Same-pool swap_remove changes an observed index; stale index must fail closed.
    let mut pools = Pools::new(&g);
    pools.release(0, 0, g.full);
    pools.release(0, 1, g.full);
    let stale = pools.observe(1);
    let observed = pools.observe(0);
    assert_eq!(pools.allocate_observed(0, 0, observed, g.full), 0);
    pools.validate(g.full);
    let _ = pools.allocate_observed(0, 1, stale, g.full);
    assert_eq!(pools.operations.failed_revalidations, 1);
    pools.validate(g.full);
    // Foreign donation changes the observed free owner without consuming its credit.
    let mut pools = Pools::new(&g);
    pools.release(0, 0, g.full);
    pools.release(1, 2, g.full);
    let stale = pools.observe(0);
    let observed = pools.observe(2);
    let foreign_count = pools.free[1].len();
    assert_eq!(pools.allocate_observed(0, 2, observed, g.full), 2);
    assert_eq!(pools.free[1].len(), foreign_count);
    assert!(matches!(pools.slots[0], Slot::Free { pool: 1, .. }));
    pools.validate(g.full);
    let _ = pools.allocate_observed(0, 0, stale, g.full);
    assert_eq!(pools.operations.failed_revalidations, 1);
    pools.validate(g.full);
    // A competing allocation consumes an observed target; a source retirement is not a retry.
    let mut pools = Pools::new(&g);
    pools.release(0, 0, g.full);
    let stale = pools.observe(0);
    assert_eq!(pools.allocate_observed(0, 3, None, g.full), 0);
    pools.release(1, 2, g.full);
    let physical = pools.allocate_observed(1, 0, stale, g.full);
    assert_ne!(physical, 0);
    assert_eq!(pools.operations.failed_revalidations, 1);
    pools.validate(g.full);
    // A negative observation before retirement remains a miss, rather than waiting for a home.
    let mut pools = Pools::new(&g);
    let observed = pools.observe(0);
    assert!(observed.is_none());
    pools.release(0, 0, g.full);
    let _ = pools.allocate_observed(1, 0, observed, g.full);
    assert_eq!(pools.operations.failed_revalidations, 0);
    pools.validate(g.full);
}

fn tiny_fixtures() -> usize {
    let fixtures = [
        Case { name: "shared_boundaries_tail", records: 259, block: 8, shards: 3,
            radix: 4, shift: 62, pattern: Pattern::SplitMix },
        Case { name: "record_transitions", records: 1027, block: 4, shards: 4,
            radix: 4, shift: 62, pattern: Pattern::SplitMix },
        Case { name: "one_bucket_empty_segments", records: 263, block: 8, shards: 3,
            radix: 4, shift: 62, pattern: Pattern::OneBucket },
        Case { name: "tiny_excess_shards", records: 3, block: 8, shards: 8,
            radix: 4, shift: 62, pattern: Pattern::SplitMix },
    ];
    let schedules = [Schedule::RoundRobin, Schedule::SerialShards, Schedule::RandomBlocks];
    let mut count = 0;
    for case in fixtures {
        let g = Geometry::new(case);
        let mut baseline = None;
        for schedule in schedules {
            for record_quantum in [false, true] {
                for policy in [false, true] {
                    let mut model = Model::new(&g);
                    model.run(policy, schedule, record_quantum);
                    let result = topology(&model.directory, &g);
                    assert_eq!(result.fixed, model.stats.actual_home);
                    if !policy {
                        if let Some(ref expected) = baseline { assert_eq!(&model.directory, expected); }
                        else { baseline = Some(model.directory.clone()); }
                    }
                    count += 1;
                }
            }
        }
    }
    count
}

fn golden(result: &Topology, g: &Geometry) {
    let (boundary, paths, cycles, largest, copies) = match g.case.block {
        128 => (2037, 2165, 0, 266, 9_999_872),
        64 => (2016, 2176, 1, 621, 10_000_000),
        _ => panic!("not a golden case"),
    };
    assert_eq!(g.boundary, boundary);
    assert_eq!(result.paths, paths);
    assert_eq!(result.cycles, cycles);
    assert_eq!(result.fixed, 1);
    assert_eq!(result.largest, largest);
    assert_eq!(result.natural_copies, copies);
    assert!(!result.fallback);
}

fn print_result(model: &Model<'_>, result: &Topology, baseline: &Topology, schedule: Schedule, policy: bool) {
    let g = model.geometry;
    let c = g.case;
    let s = &model.stats;
    let o = &model.pools.operations;
    let saved = baseline.natural_copies as i64 - result.natural_copies as i64;
    // This is a conservative requested-capacity sum, not allocator RSS or a native implementation.
    let topology_bytes = g.total * size_of::<bool>() + g.full * size_of::<bool>();
    let metadata_bytes = model.heap_bytes() + topology_bytes;
    println!(concat!(
        "{{\"case\":\"{}\",\"schedule\":\"{}\",\"policy\":\"{}\",",
        "\"records\":{},\"block\":{},\"shards\":{},\"radix\":{},",
        "\"boundary_blocks\":{},\"scratch_records\":{},",
        "\"body_allocations\":{},\"boundary_fragments\":{},",
        "\"home_free_same\":{},\"home_free_foreign\":{},\"home_unread\":{},\"home_occupied\":{},",
        "\"actual_home_placements\":{},\"home_fraction\":{:.9},",
        "\"paths\":{},\"cycles\":{},\"fixed\":{},\"largest\":{},\"fallback_required\":{},",
        "\"natural_restore_copies\":{},\"saved_natural_copies\":{},\"saved_fraction\":{:.9},",
        "\"free_input_inventory_mean_at_allocation\":{:.6},\"free_input_max\":{},\"free_total_max\":{},",
        "\"minimum_requester_pool_after_allocate\":{},\"failed_revalidations\":{},",
        "\"location_reads\":{},\"location_writes\":{},\"pool_pops\":{},\"pool_pushes\":{},",
        "\"pool_swap_removes\":{},\"pool_entry_replacements\":{},",
        "\"local_transactions\":{},\"two_pool_transactions\":{},\"lock_equivalent_acquisitions\":{},",
        "\"progress_body_allocations\":{:?},\"progress_home_placements\":{:?},",
        "\"model_capacity_peak_bytes\":{},\"slot_size_bytes\":{},\"cursor_size_bytes\":{},",
        "\"native_location_words_bytes_only\":{},\"directory_hash\":\"{:016x}\"}}"
    ), c.name, schedule.name(), if policy { "credit_exchange" } else { "baseline_lifo" },
        c.records, c.block, c.shards, c.radix, g.boundary,
        (g.boundary + c.shards * c.radix) * c.block, s.bodies, s.boundaries,
        s.free_same, s.free_foreign, s.unread, s.occupied, s.actual_home,
        s.actual_home as f64 / s.bodies.max(1) as f64,
        result.paths, result.cycles, result.fixed, result.largest, result.fallback,
        result.natural_copies, saved, saved as f64 / baseline.natural_copies as f64,
        s.free_input_sum as f64 / s.inventory_samples.max(1) as f64,
        s.free_input_max, s.free_total_max, model.pools.minimum_pool, o.failed_revalidations,
        o.location_reads, o.location_writes, o.pool_pops, o.pool_pushes,
        o.pool_swap_removes, o.pool_entry_replacements, o.local_transactions,
        o.two_pool_transactions, o.lock_equivalent_acquisitions,
        s.progress_bodies, s.progress_homes, metadata_bytes, size_of::<Slot>(), size_of::<Cursor>(),
        g.total * size_of::<u64>(), result.directory_hash);
}

fn main() {
    assert_eq!(std::env::args().nth(1).as_deref(), Some("--fixed-suite"), "use --fixed-suite only");
    transaction_fixtures();
    let fixtures = tiny_fixtures();
    println!("{{\"kind\":\"fixture_summary\",\"transaction_fixtures\":4,\"small_runs\":{fixtures},\"passed\":true}}");
    println!("{{\"kind\":\"scope\",\"payload_records_stored\":0,\"native_locks\":false,\"schedule_quantum\":\"one_complete_source_block\",\"rayon_trace\":false,\"timing_claim\":false}}");
    let mut investment_gate = true;
    for (name, block) in [("u64", 128), ("row16", 64)] {
        let g = Geometry::new(Case { name, records: 10_000_000, block, shards: 8,
            radix: 256, shift: 56, pattern: Pattern::SplitMix });
        let mut baseline_signature = None;
        for schedule in [Schedule::RoundRobin, Schedule::SerialShards, Schedule::RandomBlocks] {
            let baseline = {
                let mut model = Model::new(&g);
                model.run(false, schedule, false);
                let result = topology(&model.directory, &g);
                golden(&result, &g);
                if let Some(ref expected) = baseline_signature { assert_eq!(&result, expected); }
                else { baseline_signature = Some(result.clone()); }
                print_result(&model, &result, &result, schedule, false);
                result
            };
            let mut model = Model::new(&g);
            model.run(true, schedule, false);
            let result = topology(&model.directory, &g);
            assert_eq!(result.fixed, model.stats.actual_home);
            print_result(&model, &result, &baseline, schedule, true);
            if !matches!(schedule, Schedule::SerialShards) {
                investment_gate &= !result.fallback
                    && result.natural_copies * 4 <= baseline.natural_copies * 3;
            }
            // A fallback changes the actual restoration algorithm. Stop rather than claim its savings.
            assert!(!result.fallback, "candidate requires fallback: natural copy count is not an actual restore claim");
        }
    }
    println!("{{\"kind\":\"complete\",\"golden_baselines\":6,\"policy_cases\":6,\"passed\":true,\"minimum_savings_fraction_required\":0.25,\"investment_gate_passed\":{investment_gate}}}");
}
