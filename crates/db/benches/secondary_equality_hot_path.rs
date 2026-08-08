//! Fixed V3/V4 secondary-equality hot-path benchmark.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use db::production_coverage::{
    benchmark_million_sequential_id_bitmap, SecondaryEqualityHotPathFixture,
    SecondaryEqualityInsertMode, SecondaryEqualityReadMode,
};
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
    read_sequential: db::production_coverage::SecondaryEqualityReadSample,
    read_concurrent: db::production_coverage::SecondaryEqualityReadSample,
    concurrent: db::production_coverage::SecondaryEqualityInsertSample,
    concurrent_inspection: db::production_coverage::SecondaryEqualityInspection,
    million_sequential_id_bitmap: db::production_coverage::SecondaryEqualityMillionBitmapSample,
}

fn assert_v4_shape(inspection: &db::production_coverage::SecondaryEqualityInspection) {
    assert_eq!(inspection.physical_secondary_rows, 50);
    assert_eq!(inspection.v3_nonunique_rows, 0);
    assert_eq!(inspection.v4_bitmap_rows, 50);
    assert_eq!(inspection.minimum_bitmap_cardinality, 1_000);
    assert_eq!(inspection.maximum_bitmap_cardinality, 1_000);
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
        let sequential = sequential_fixture
            .insert(SecondaryEqualityInsertMode::Sequential)
            .await
            .expect("sequential hot-path insertions succeed");
        let sequential_inspection = sequential_fixture
            .inspect()
            .await
            .expect("sequential hot-path inspection succeeds");
        assert_v4_shape(&sequential_inspection);
        sequential_fixture
            .prepare_read()
            .await
            .expect("sequential hot-path lookup warmup succeeds");
        let read_sequential = sequential_fixture
            .read(SecondaryEqualityReadMode::Sequential)
            .await
            .expect("sequential hot-path lookups succeed");
        let read_concurrent = sequential_fixture
            .read(SecondaryEqualityReadMode::Concurrent)
            .await
            .expect("concurrent hot-path lookups succeed");
        sequential_fixture
            .close()
            .await
            .expect("sequential hot-path fixture closes");

        let concurrent_fixture =
            SecondaryEqualityHotPathFixture::open("secondary-equality-hot-path-concurrent")
                .await
                .expect("concurrent hot-path fixture opens");
        let concurrent = concurrent_fixture
            .insert(SecondaryEqualityInsertMode::Concurrent)
            .await
            .expect("concurrent hot-path insertions succeed");
        let concurrent_inspection = concurrent_fixture
            .inspect()
            .await
            .expect("concurrent hot-path inspection succeeds");
        assert_v4_shape(&concurrent_inspection);
        concurrent_fixture
            .close()
            .await
            .expect("concurrent hot-path fixture closes");

        let allocation_fixture =
            SecondaryEqualityHotPathFixture::open("secondary-equality-hot-path-allocations")
                .await
                .expect("allocation hot-path fixture opens");
        ALLOCATOR.start();
        allocation_fixture
            .insert(SecondaryEqualityInsertMode::Sequential)
            .await
            .expect("allocation sequential insertions succeed");
        let (sequential_allocations, sequential_allocated_bytes) = ALLOCATOR.finish();
        allocation_fixture
            .prepare_read()
            .await
            .expect("allocation lookup warmup succeeds");
        ALLOCATOR.start();
        allocation_fixture
            .read(SecondaryEqualityReadMode::Sequential)
            .await
            .expect("allocation sequential lookups succeed");
        let (sequential_read_allocations, sequential_read_allocated_bytes) = ALLOCATOR.finish();
        ALLOCATOR.start();
        allocation_fixture
            .read(SecondaryEqualityReadMode::Concurrent)
            .await
            .expect("allocation concurrent lookups succeed");
        let (concurrent_read_allocations, concurrent_read_allocated_bytes) = ALLOCATOR.finish();
        allocation_fixture
            .close()
            .await
            .expect("allocation hot-path fixture closes");

        let concurrent_allocation_fixture = SecondaryEqualityHotPathFixture::open(
            "secondary-equality-hot-path-concurrent-allocations",
        )
        .await
        .expect("concurrent allocation hot-path fixture opens");
        ALLOCATOR.start();
        concurrent_allocation_fixture
            .insert(SecondaryEqualityInsertMode::Concurrent)
            .await
            .expect("allocation concurrent insertions succeed");
        let (concurrent_allocations, concurrent_allocated_bytes) = ALLOCATOR.finish();
        concurrent_allocation_fixture
            .close()
            .await
            .expect("concurrent allocation hot-path fixture closes");

        let sequential =
            sequential.with_allocations(sequential_allocations, sequential_allocated_bytes);
        let read_sequential = read_sequential
            .with_allocations(sequential_read_allocations, sequential_read_allocated_bytes);
        let read_concurrent = read_concurrent
            .with_allocations(concurrent_read_allocations, concurrent_read_allocated_bytes);
        let concurrent =
            concurrent.with_allocations(concurrent_allocations, concurrent_allocated_bytes);

        BenchmarkReport {
            sequential,
            sequential_inspection,
            read_sequential,
            read_concurrent,
            concurrent,
            concurrent_inspection,
            million_sequential_id_bitmap: benchmark_million_sequential_id_bitmap(),
        }
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("benchmark report serializes")
    );
}
