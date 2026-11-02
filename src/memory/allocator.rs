use super::frame::{PhysFrame, UsableFrameIter};

/// A simple, monotonic physical-frame allocator.
///
/// Frames are returned once and are never returned to the pool. This is suitable
/// for early kernel bring-up; a reclaiming allocator will replace it later.
pub struct PhysicalFrameAllocator {
    frames: UsableFrameIter,
}

impl PhysicalFrameAllocator {
    /// Creates an allocator over Limine's usable physical memory regions.
    ///
    /// # Safety
    /// Must only be called after Limine has transferred control to the kernel.
    /// The bootloader memory map must remain mapped and valid while the allocator
    /// is in use.
    pub unsafe fn new() -> Option<Self> {
        // SAFETY: The caller guarantees that Limine's memory map remains valid.
        let frames = unsafe { UsableFrameIter::new()? };
        Some(Self { frames })
    }

    /// Returns the next available 4 KiB physical frame.
    ///
    /// The returned frame is not mapped or cleared by this function. The caller
    /// must establish an appropriate virtual mapping before accessing its memory.
    pub fn allocate_frame(&mut self) -> Option<PhysFrame> {
        self.frames.next()
    }

    /// Allocates a physically contiguous run of 4 KiB frames.
    ///
    /// Frames consumed while searching are reserved even if no sufficiently large
    /// run is found. The returned run is never reclaimed by this early allocator.
    pub fn allocate_contiguous_frames(&mut self, count: usize) -> Option<PhysFrame> {
        const PAGE_SIZE: u64 = 4096;

        if count == 0 {
            return None;
        }

        let mut run_start = 0;
        let mut previous = 0;
        let mut run_length = 0_usize;

        for frame in self.frames.by_ref() {
            if run_length != 0 && previous.checked_add(PAGE_SIZE) == Some(frame.start_address) {
                run_length += 1;
            } else {
                run_start = frame.start_address;
                run_length = 1;
            }

            previous = frame.start_address;
            if run_length == count {
                return Some(PhysFrame {
                    start_address: run_start,
                });
            }
        }

        None
    }
}
