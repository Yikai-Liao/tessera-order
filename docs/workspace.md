# Reusing scratch memory with `Workspace`

> This describes the planned 0.1 API. The examples will become compiled documentation tests once the engine is ported.

Tessera needs scratch memory while it sorts: O(√n) records for the block partition, plus one tile of up to 64 KiB of records and its cursor table. The free functions such as `stable_sort_by_key` allocate this memory at the start of each call and free it before returning.

A `Workspace<T>` holds that scratch memory between calls instead. The first sort allocates what it needs; later sorts reuse the capacity and allocate only when they need more. Results are identical either way: a workspace changes who owns the scratch memory and how long it lives, never the output.

Use a workspace when you sort many slices of the same record type, for example one batch of rows at a time. For a single sort, the free functions are simpler and just as fast.

## API

```rust
impl<T> Workspace<T> {
    pub fn new() -> Self;

    pub fn stable_sort_by_key<K: RadixKey>(&mut self, values: &mut [T], key: impl FnMut(&T) -> K);
    pub fn stable_sort_by_key_copy<K: RadixKey>(&mut self, values: &mut [T], key: impl FnMut(&T) -> K)
    where
        T: Copy;
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

`Row` owns a `String`, so it uses the general path, which moves records without ever duplicating them.

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

### Plain-data records: the `_copy` path

When the record type is `Copy`, the `_copy` method lets the engine keep byte copies of records in scratch, which saves bookkeeping. Typical cases are key + row-number pairs:

```rust
#[derive(Clone, Copy)]
struct Entry {
    key: u64,
    row: u32,
}

let mut ws = Workspace::<Entry>::new();
for entries in batches.iter_mut() {
    ws.stable_sort_by_key_copy(entries, |e| e.key);
}
```

`stable_sort_by_key` also works for `Entry`; it just takes the general path. A record that is not `Copy` cannot call the `_copy` method; this is checked at compile time.

### Sorting keys directly

```rust
let mut ws = Workspace::<u32>::new();
for column in columns.iter_mut() {
    ws.stable_sort(column);
}
```

### Bounding retained memory

A workspace keeps the largest capacity it has used until you release it. After one large sort, it holds that capacity even while it sorts small slices. Long-running services can cap it:

```rust
const RETAIN_LIMIT: usize = 4 << 20; // 4 MiB

fn handle(ws: &mut Workspace<Entry>, entries: &mut [Entry]) {
    ws.stable_sort_by_key_copy(entries, |e| e.key);
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
                ws.stable_sort_by_key_copy(batch, |e| e.key);
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
    WS.with_borrow_mut(|ws| ws.stable_sort_by_key_copy(entries, |e| e.key));
}
```

## What a workspace keeps

| Kept between calls | Not kept between calls |
|---|---|
| Allocated capacity | Records, or any knowledge of which scratch slots hold records |

Every call starts by treating all scratch memory as uninitialized. Bytes left by an earlier sort are never read as records. This matters for the `_copy` path, which within one call remembers which part of scratch already holds valid copies; that state is reset at the start of each call.

If a key function panics, the slice still holds every record, and the workspace remains usable for the next sort.

## Design notes

- **Why `Workspace<T>` and not an untyped buffer.** Scratch holds records, so typing it by `T` lets the compiler handle size and alignment. The cost is that one workspace serves one record type. An untyped byte workspace shared across record types is possible but needs more unsafe code for layout; it may be revisited after 0.1. Most call sites rely on type inference and would not change.
- **Why methods and not `*_with_workspace` functions.** Each method mirrors a free function, so there is one set of names to learn. A free function behaves like a temporary workspace that is dropped when the call returns.
