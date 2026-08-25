use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};

pub struct TrackingAllocator;

static CURRENT_BYTES: AtomicIsize = AtomicIsize::new(0);
static PEAK_BYTES: AtomicUsize = AtomicUsize::new(0);
static ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);
static DEALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);
static TOTAL_BYTES_ALLOCATED: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc(layout);
        if !ptr.is_null() {
            let size = layout.size();
            let current = CURRENT_BYTES.fetch_add(size as isize, Ordering::SeqCst) + (size as isize);
            ALLOC_COUNT.fetch_add(1, Ordering::SeqCst);
            TOTAL_BYTES_ALLOCATED.fetch_add(size, Ordering::SeqCst);

            let mut peak = PEAK_BYTES.load(Ordering::SeqCst);
            while (current as usize) > peak {
                match PEAK_BYTES.compare_exchange_weak(
                    peak,
                    current as usize,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                ) {
                    Ok(_) => break,
                    Err(actual) => peak = actual,
                }
            }
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
        let size = layout.size();
        CURRENT_BYTES.fetch_sub(size as isize, Ordering::SeqCst);
        DEALLOC_COUNT.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Debug, Clone, Copy, Default)]
#[allow(dead_code)]
pub struct MemorySnapshot {
    pub current_bytes: isize,
    pub peak_bytes: usize,
    pub alloc_count: usize,
    pub dealloc_count: usize,
    pub total_bytes_allocated: usize,
}

#[derive(Debug, Clone, Copy, Default)]
#[allow(dead_code)]
pub struct MemoryDelta {
    pub peak_bytes: usize,
    pub bytes_allocated: usize,
    pub alloc_ops: usize,
    pub dealloc_ops: usize,
    pub net_bytes: isize,
}

impl TrackingAllocator {
    pub fn reset_peak() {
        let cur = CURRENT_BYTES.load(Ordering::SeqCst).max(0) as usize;
        PEAK_BYTES.store(cur, Ordering::SeqCst);
    }

    pub fn snapshot() -> MemorySnapshot {
        MemorySnapshot {
            current_bytes: CURRENT_BYTES.load(Ordering::SeqCst),
            peak_bytes: PEAK_BYTES.load(Ordering::SeqCst),
            alloc_count: ALLOC_COUNT.load(Ordering::SeqCst),
            dealloc_count: DEALLOC_COUNT.load(Ordering::SeqCst),
            total_bytes_allocated: TOTAL_BYTES_ALLOCATED.load(Ordering::SeqCst),
        }
    }

    pub fn profile<F, R>(f: F) -> (R, MemoryDelta)
    where
        F: FnOnce() -> R,
    {
        Self::reset_peak();
        let before = Self::snapshot();
        let result = f();
        let after = Self::snapshot();

        let peak_during = after.peak_bytes.saturating_sub(before.current_bytes.max(0) as usize);
        let bytes_allocated = after.total_bytes_allocated.saturating_sub(before.total_bytes_allocated);
        let alloc_ops = after.alloc_count.saturating_sub(before.alloc_count);
        let dealloc_ops = after.dealloc_count.saturating_sub(before.dealloc_count);
        let net_leak = after.current_bytes - before.current_bytes;

        (
            result,
            MemoryDelta {
                peak_bytes: peak_during,
                bytes_allocated,
                alloc_ops,
                dealloc_ops,
                net_bytes: net_leak,
            },
        )
    }
}