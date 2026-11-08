---
type: Playbook
title: Workspace reuse
description: Reuse scratch across calls and bound retained memory.
tags: [api, rust, memory]
---

# Reusing scratch memory with `Workspace`

Tessera needs scratch memory while it sorts: O(√n) records for the block partition, one tile of up to 64 KiB of records, up to two bytes per record for user-key bucket IDs, and the order probe's encoded keys. Cursor tables live on the stack. The free functions such as `stable_sort_by_key` allocate this memory during each call and free it before returning.

A `Workspace<T>` holds that scratch memory between calls instead. The first sort allocates what it needs; later sorts reuse the capacity and allocate only when they need more. Results are identical either way: a workspace changes who owns the scratch memory and how long it lives, never the output.

Use a workspace when you sort many slices of the same record type, for example one batch of rows at a time. For a single sort, the free functions are simpler.

## API

```rust
impl<T> Workspace<T> {
    pub fn new() -> Self;

    pub fn stable_sort_by_key<K: RadixKey>(&mut self, values: &mut [T], key: impl FnMut(&T) -> K);
    pub fn stable_sort(&mut self, values: &mut [T])
    where
        T: RadixKey;

    pub fn retained_bytes(&self) -> usize;
    pub fn shrink_to_fit(&mut self);
}

impl<T> Default for Workspace<T> { /* same as new() */ }
```

Each sorting method behaves exactly like the free function of the same name. `Workspace::new()` does not allocate.

## Examples

### Sorting batches

```rust
use tessera_sort::Workspace;

struct Row {
    ts: i64,
    user: String,
}

fn sort_batches(batches: &mut [Vec<Row>]) {
    let mut ws = Workspace::new();
    for batch in batches {
        // Reuses the capacity left by the previous batch.
        ws.stable_sort_by_key(batch, |row| row.ts);
    }
}
```

`Row` owns a `String`. Records are moved, never duplicated, so any record type works.

### Different keys, one workspace

The workspace is typed by the record, not by the key, so the same workspace can sort by any supported key:

```rust
use std::cmp::Reverse;

let mut ws = Workspace::<Row>::new();

// Newest first.
ws.stable_sort_by_key(&mut rows, |row| Reverse(row.ts));

// Two-level order: by name length, ties by timestamp. Because each sort is
// stable, sort by the secondary key first and the primary key last.
ws.stable_sort_by_key(&mut rows, |row| row.ts);         // i64
ws.stable_sort_by_key(&mut rows, |row| row.user.len()); // usize
```

### Key + row-number records

Plain-data records, such as a key paired with a row number, are the most common radix-sort input:

```rust
#[derive(Clone, Copy)]
struct Entry {
    key: u64,
    row: u32,
}

let mut ws = Workspace::<Entry>::new();
for entries in batches.iter_mut() {
    ws.stable_sort_by_key(entries, |e| e.key);
}
```

### Sorting keys directly

```rust
let mut ws = Workspace::<u32>::new();
for column in columns.iter_mut() {
    ws.stable_sort(column);
}
```

### Bounding retained memory

A workspace keeps the largest capacity it has used until you release it, so after one large sort it still holds that capacity while sorting small slices. (A key-function panic may drop part of the block-partition capacity; the next call allocates it again.) Long-running services can cap it:

```rust
const RETAIN_LIMIT: usize = 4 << 20; // 4 MiB

fn handle(ws: &mut Workspace<Entry>, entries: &mut [Entry]) {
    ws.stable_sort_by_key(entries, |e| e.key);
    if ws.retained_bytes() > RETAIN_LIMIT {
        ws.shrink_to_fit();
    }
}
```

`retained_bytes` reports heap capacity only. Fixed stack storage, up to 32 KiB per tile-local MSD level, is not included.

### One workspace per thread

Sorting takes `&mut self`, so a workspace serves one sort at a time. Give each thread its own:

```rust
std::thread::scope(|s| {
    for share in batches.chunks_mut(batches_per_thread) {
        s.spawn(move || {
            let mut ws = Workspace::<Entry>::new();
            for batch in share {
                ws.stable_sort_by_key(batch, |e| e.key);
            }
        });
    }
});
```

Or keep one in thread-local storage:

```rust
use std::cell::RefCell;

thread_local! {
    static WS: RefCell<Workspace<Entry>> = RefCell::new(Workspace::new());
}

fn sort_entries(entries: &mut [Entry]) {
    WS.with_borrow_mut(|ws| ws.stable_sort_by_key(entries, |e| e.key));
}
```

## What a workspace keeps

| Kept between calls | Not kept between calls |
|---|---|
| Allocated capacity | Records, or any knowledge of which scratch slots hold records |

Every call starts by treating all scratch memory as uninitialized. Bytes left by an earlier sort are never read as records.

If a key function panics, the slice still holds every record, and the workspace remains usable for the next sort. Without the `std` feature, a panic during block partitioning aborts, even if the host enables unwinding; panics elsewhere may unwind with every record preserved.

## Design notes

- **Why `Workspace<T>` and not an untyped buffer.** Scratch holds records, so typing it by `T` lets the compiler handle size and alignment. The cost is that one workspace serves one record type. An untyped byte workspace shared across record types is possible but needs more unsafe code for layout; it may be revisited after 0.1. Most call sites rely on type inference and would not change.
- **Why methods and not `*_with_workspace` functions.** Each method mirrors a free function, so there is one set of names to learn. A free function behaves like a temporary workspace that is dropped when the call returns.
