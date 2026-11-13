use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use exact_kernel::arena::NodeArena;
use exact_kernel::layout::{LayoutMirror, LayoutTree, Unmirrored};
use exact_kernel::selector::SelectorIndex;
use exact_kernel::txn::{self, Target};
use exact_kernel::{NodeType, Op};

#[derive(Clone, Copy, Default, Debug)]
struct Allocations {
    calls: usize,
    bytes: usize,
    live: isize,
    peak: isize,
}

thread_local! {
    static ALLOCATIONS: Cell<Allocations> = const { Cell::new(Allocations {
        calls: 0, bytes: 0, live: 0, peak: 0,
    }) };
}

fn record(bytes: usize, freed: usize, allocation: bool) {
    let _ = ALLOCATIONS.try_with(|cell| {
        let mut count = cell.get();
        count.calls += usize::from(allocation);
        count.bytes += bytes;
        count.live += bytes as isize - freed as isize;
        count.peak = count.peak.max(count.live);
        cell.set(count);
    });
}

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            record(layout.size(), 0, true);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        record(0, layout.size(), false);
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let next = unsafe { System.realloc(ptr, layout, size) };
        if !next.is_null() {
            record(size, layout.size(), true);
        }
        next
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn measured<T>(f: impl FnOnce() -> T) -> (T, Allocations) {
    let start = ALLOCATIONS.with(|cell| {
        let start = cell.get().live;
        cell.set(Allocations {
            live: start,
            peak: start,
            ..Allocations::default()
        });
        start
    });
    let result = f();
    let mut count = ALLOCATIONS.with(Cell::get);
    count.live -= start;
    count.peak -= start;
    (result, count)
}

fn create(id: u32) -> Op {
    Op::CreateView {
        id,
        node_type: NodeType::View,
    }
}

#[test]
fn create_move_destroy_churn_allocations() {
    let mut measurements = Vec::new();
    for mirrored in [false, true] {
        let mut arena = NodeArena::new();
        let mut layout: Box<dyn LayoutMirror> = if mirrored {
            Box::new(LayoutTree::new())
        } else {
            Box::new(Unmirrored)
        };
        let mut selectors = SelectorIndex::default();
        let mut apply = |arena: &mut NodeArena, ops: &[Op], batch| {
            txn::apply(
                Target {
                    arena,
                    layout: layout.as_mut(),
                    selectors: &mut selectors,
                },
                ops,
                batch,
                0,
                batch,
            )
            .unwrap()
        };
        let mut initial: Vec<_> = (1..=3).chain(10..230).map(create).collect();
        initial.extend([
            Op::SetChildren {
                id: 1,
                children: vec![2, 3],
            },
            Op::SetChildren {
                id: 2,
                children: (10..230).collect(),
            },
            Op::AttachRoot { id: 1 },
        ]);
        apply(&mut arena, &initial, 0);
        let mut total = Allocations::default();
        for round in 0..500u32 {
            let first = 10 + round * 20;
            let next = first + 220;
            let destination = if round % 2 == 0 { 3 } else { 2 };
            let mut ops: Vec<_> = (next..next + 20).map(create).collect();
            ops.push(Op::SetChildren {
                id: destination,
                children: (first..next + 20).collect(),
            });
            ops.extend((first..first + 20).map(|id| Op::DestroyView { id }));
            let destroyed: Vec<_> = (first..first + 20)
                .map(|id| arena.key_of(id).unwrap())
                .collect();
            let (receipt, count) = measured(|| apply(&mut arena, &ops, u64::from(round) + 1));
            total.calls += count.calls;
            total.bytes += count.bytes;
            total.peak = total.peak.max(count.peak);
            assert_eq!(receipt.created.len(), 20);
            assert_eq!(receipt.destroyed, destroyed);
            assert_eq!(arena.live_count(), 223);
            assert_eq!(
                receipt
                    .created
                    .iter()
                    .map(|key| arena.local_id(key.index))
                    .collect::<Vec<_>>(),
                (next..next + 20).collect::<Vec<_>>()
            );
        }
        eprintln!(
            "mirrored={mirrored}: {} allocations, {} requested bytes, {} peak live bytes",
            total.calls, total.bytes, total.peak,
        );
        measurements.push((mirrored, total));
    }
    // Without reservations: 536,507 / 545,519 allocations, 191,308,872 /
    // 199,982,068 requested bytes, and 172,484 / 246,788 peak live bytes.
    // Leave room for small changes while retaining the allocation reduction
    // and keeping peak memory within 1% of the unreserved transaction.
    for (mirrored, total) in measurements {
        let (calls, bytes, peak) = if mirrored {
            (540_000, 198_000_000, 249_000)
        } else {
            (531_000, 190_000_000, 174_000)
        };
        assert!(total.calls < calls, "{total:?}");
        assert!(total.bytes < bytes, "{total:?}");
        assert!(total.peak < peak, "{total:?}");
    }
}
