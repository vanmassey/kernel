use crate::boot::limine::{self, MemoryMapEntries};

const PAGE_SIZE: u64 = 4096;
const LIMINE_MEMMAP_USABLE: u64 = 0;

/// A page-aligned physical memory frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhysFrame {
    pub start_address: u64,
}

/// Iterates over 4 KiB physical frames described as usable by Limine.
pub struct UsableFrameIter {
    regions: MemoryMapEntries,
    current: Option<(u64, u64)>,
}

impl UsableFrameIter {
    /// Creates a frame iterator from the bootloader memory map.
    ///
    /// # Safety
    /// Must only be called after Limine has transferred control to the kernel.
    /// The bootloader memory map must remain mapped and valid for the allocator's lifetime.
    pub unsafe fn new() -> Option<Self> {
        // SAFETY: The caller guarantees that Limine's boot information remains valid.
        let regions = unsafe { limine::memory_map_entries()? };
        Some(Self {
            regions,
            current: None,
        })
    }
}

impl Iterator for UsableFrameIter {
    type Item = PhysFrame;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some((next_address, end_address)) = self.current {
                if next_address < end_address {
                    self.current = Some((next_address.saturating_add(PAGE_SIZE), end_address));
                    return Some(PhysFrame {
                        start_address: next_address,
                    });
                }
                self.current = None;
            }

            let region = self.regions.next()?;
            if region.entry_type != LIMINE_MEMMAP_USABLE {
                continue;
            }

            let Some(region_end) = region.base.checked_add(region.length) else {
                continue;
            };
            let Some(aligned_start) = region.base.checked_add(PAGE_SIZE - 1) else {
                continue;
            };
            let aligned_start = aligned_start & !(PAGE_SIZE - 1);
            let aligned_end = region_end & !(PAGE_SIZE - 1);

            if aligned_start < aligned_end {
                self.current = Some((aligned_start, aligned_end));
            }
        }
    }
}
