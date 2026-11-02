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
}
