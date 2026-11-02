use core::alloc::{GlobalAlloc, Layout};
use core::ptr;
use core::sync::atomic::{AtomicUsize, Ordering};

/// A small, early-boot heap. Allocations are aligned and never reclaimed.
pub const HEAP_SIZE: usize = 256 * 1024;

struct BumpAllocator {
    next: AtomicUsize,
    end: AtomicUsize,
}

unsafe impl Sync for BumpAllocator {}

impl BumpAllocator {
    const fn new() -> Self {
        Self {
            next: AtomicUsize::new(0),
            end: AtomicUsize::new(0),
        }
    }

    fn initialise(&self, start: *mut u8, size: usize) -> bool {
        let start = start as usize;
        let Some(end) = start.checked_add(size) else {
            return false;
        };
        if start == 0 || size == 0 || self.next.load(Ordering::Acquire) != 0 {
            return false;
        }

        self.end.store(end, Ordering::Relaxed);
        self.next
            .compare_exchange(0, start, Ordering::Release, Ordering::Relaxed)
            .is_ok()
    }
}

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let alignment = layout.align();
        let size = layout.size();

        if size == 0 {
            return alignment as *mut u8;
        }

        let mut current = self.next.load(Ordering::Relaxed);
        loop {
            if current == 0 {
                return ptr::null_mut();
            }

            let Some(aligned) = current.checked_add(alignment - 1) else {
                return ptr::null_mut();
            };
            let aligned = aligned & !(alignment - 1);
            let Some(next) = aligned.checked_add(size) else {
                return ptr::null_mut();
            };
            if next > self.end.load(Ordering::Acquire) {
                return ptr::null_mut();
            }

            match self.next.compare_exchange_weak(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => return aligned as *mut u8,
                Err(actual) => current = actual,
            }
        }
    }

    unsafe fn dealloc(&self, _pointer: *mut u8, _layout: Layout) {
        // The early heap is monotonic; reclamation will be added with the full allocator.
    }
}

#[global_allocator]
static HEAP_ALLOCATOR: BumpAllocator = BumpAllocator::new();

/// Installs a heap over a contiguous, writable region already mapped into the HHDM.
pub fn initialise(start: *mut u8, size: usize) -> bool {
    HEAP_ALLOCATOR.initialise(start, size)
}

/// Exercises allocation, alignment, and memory access without requiring the alloc crate.
pub fn self_test() -> bool {
    let Ok(layout) = Layout::from_size_align(64, 16) else {
        return false;
    };

    // SAFETY: The global allocator returns a unique region of at least layout.size() bytes.
    let allocation = unsafe { HEAP_ALLOCATOR.alloc(layout) };
    if allocation.is_null() || allocation as usize % layout.align() != 0 {
        return false;
    }

    for index in 0..layout.size() {
        // SAFETY: The allocation is valid for 64 bytes and index is in bounds.
        unsafe { ptr::write_volatile(allocation.add(index), (index as u8) ^ 0x5a) };
    }

    for index in 0..layout.size() {
        // SAFETY: The same allocation remains owned by this test and index is in bounds.
        if unsafe { ptr::read_volatile(allocation.add(index)) } != (index as u8) ^ 0x5a {
            return false;
        }
    }

    true
}
