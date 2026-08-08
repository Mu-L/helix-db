//! Fixed V3/V4 secondary-equality hot-path benchmark.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use db::production_coverage::{SecondaryEqualityHotPathFixture, SecondaryEqualityInsertMode};
use serde::Serialize;

struct TrackingAllocator {
    enabled: AtomicBool,
    allocations: AtomicU64,
    allocated_bytes: AtomicU64,
}

impl TrackingAllocator {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocations: AtomicU64::new(0),
            allocated_bytes: AtomicU64::new(0),
        }
    }

    fn start(&self) {
        self.allocations.store(0, Ordering::Relaxed);
        self.allocated_bytes.store(0, Ordering::Relaxed);
        self.enabled.store(true, Ordering::Release);
    }

    fn finish(&self) -> (u64, u64) {
        self.enabled.store(false, Ordering::Release);
        (
            self.allocations.load(Ordering::Relaxed),
            self.allocated_bytes.load(Ordering::Relaxed),
        )
    }
}

// SAFETY: every allocation and deallocation delegates unchanged layouts and
// pointers to the system allocator. Relaxed counters are observational only.
unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if self.enabled.load(Ordering::Acquire) {
            self.allocations.fetch_add(1, Ordering::Relaxed);
            self.allocated_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: this method preserves the caller-provided layout contract.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: the pointer and layout came from the delegated system allocation.
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if self.enabled.load(Ordering::Acquire) {
            self.allocations.fetch_add(1, Ordering::Relaxed);
            self.allocated_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: this method preserves the caller-provided layout contract.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if self.enabled.load(Ordering::Acquire) {
            self.allocations.fetch_add(1, Ordering::Relaxed);
            self.allocated_bytes
                .fetch_add(new_size as u64, Ordering::Relaxed);
        }
        // SAFETY: this method preserves the delegated reallocation contract.
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator::new();

#[derive(Serialize)]
struct BenchmarkReport {
    sequential: db::production_coverage::SecondaryEqualityInsertSample,
    sequential_inspection: db::production_coverage::SecondaryEqualityInspection,
    concurrent: db::production_coverage::SecondaryEqualityInsertSample,
    concurrent_inspection: db::production_coverage::SecondaryEqualityInspection,
}

fn main() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(32)
        .enable_all()
        .build()
        .expect("secondary equality benchmark runtime starts");
    let report = runtime.block_on(async {
        let sequential_fixture =
            SecondaryEqualityHotPathFixture::open("secondary-equality-hot-path-sequential")
                .await
                .expect("sequential hot-path fixture opens");
        ALLOCATOR.start();
        let sequential = sequential_fixture
            .insert(SecondaryEqualityInsertMode::Sequential)
            .await
            .expect("sequential hot-path insertions succeed");
        let (sequential_allocations, sequential_allocated_bytes) = ALLOCATOR.finish();
        let sequential =
            sequential.with_allocations(sequential_allocations, sequential_allocated_bytes);
        let sequential_inspection = sequential_fixture
            .inspect()
            .await
            .expect("sequential hot-path inspection succeeds");
        sequential_fixture
            .close()
            .await
            .expect("sequential hot-path fixture closes");

        let concurrent_fixture =
            SecondaryEqualityHotPathFixture::open("secondary-equality-hot-path-concurrent")
                .await
                .expect("concurrent hot-path fixture opens");
        ALLOCATOR.start();
        let concurrent = concurrent_fixture
            .insert(SecondaryEqualityInsertMode::Concurrent)
            .await
            .expect("concurrent hot-path insertions succeed");
        let (concurrent_allocations, concurrent_allocated_bytes) = ALLOCATOR.finish();
        let concurrent =
            concurrent.with_allocations(concurrent_allocations, concurrent_allocated_bytes);
        let concurrent_inspection = concurrent_fixture
            .inspect()
            .await
            .expect("concurrent hot-path inspection succeeds");
        concurrent_fixture
            .close()
            .await
            .expect("concurrent hot-path fixture closes");

        BenchmarkReport {
            sequential,
            sequential_inspection,
            concurrent,
            concurrent_inspection,
        }
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("benchmark report serializes")
    );
}
