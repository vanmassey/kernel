use super::allocator::PhysicalFrameAllocator;
use super::frame::{physical_to_virtual, PhysFrame};
use core::ptr;
use core::sync::atomic::{compiler_fence, Ordering};

const PRESENT: u64 = 1 << 0;
const HUGE_PAGE: u64 = 1 << 7;
const ADDRESS_MASK: u64 = 0x000f_ffff_ffff_f000;
const PAGE_4K: u64 = 4096;
const PAGE_2M: u64 = 2 * 1024 * 1024;
const PAGE_1G: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageSize {
    Size4KiB,
    Size2MiB,
    Size1GiB,
}

impl PageSize {
    pub const fn bytes(self) -> u64 {
        match self {
            Self::Size4KiB => PAGE_4K,
            Self::Size2MiB => PAGE_2M,
            Self::Size1GiB => PAGE_1G,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Translation {
    pub physical_address: u64,
    pub page_size: PageSize,
}

/// Reads the active CR3 register.
///
/// # Safety
/// This instruction is privileged and must only be called from ring 0.
pub unsafe fn current_cr3() -> u64 {
    let value: u64;
    // SAFETY: The caller guarantees ring 0 execution.
    unsafe {
        core::arch::asm!(
            "mov {}, cr3",
            out(reg) value,
            options(nomem, nostack, preserves_flags)
        );
    }
    value & ADDRESS_MASK
}

fn read_entry(table_physical: u64, index: usize, hhdm_offset: u64) -> Option<u64> {
    if index >= 512 || table_physical & (PAGE_4K - 1) != 0 {
        return None;
    }

    let table_virtual = physical_to_virtual(table_physical, hhdm_offset)?;
    let table = table_virtual.as_u64() as *const u64;

    // SAFETY: The caller obtains table_physical from the active page tables.
    // Limine's HHDM maps physical memory, and index is constrained to one table page.
    Some(unsafe { core::ptr::read_volatile(table.add(index)) })
}

fn entry_address(entry: u64) -> Option<u64> {
    if entry & PRESENT == 0 {
        None
    } else {
        Some(entry & ADDRESS_MASK)
    }
}

/// Translates a virtual address using the currently active four-level x86_64 page tables.
///
/// # Safety
/// Must run in ring 0 after Limine has installed a valid HHDM. Every physical page-table
/// page traversed must be accessible through that direct map. Five-level paging is rejected.
pub unsafe fn translate_current(virtual_address: u64, hhdm_offset: u64) -> Option<Translation> {
    let upper = virtual_address >> 48;
    let sign = (virtual_address >> 47) & 1;
    if !((sign == 0 && upper == 0) || (sign == 1 && upper == 0xffff)) {
        return None;
    }

    let cr4: u64;
    // SAFETY: Reading CR4 is privileged and this function's caller guarantees ring 0.
    unsafe {
        core::arch::asm!(
            "mov {}, cr4",
            out(reg) cr4,
            options(nomem, nostack, preserves_flags)
        );
    }
    const CR4_LA57: u64 = 1 << 12;
    if cr4 & CR4_LA57 != 0 {
        return None;
    }

    // SAFETY: This function is restricted to ring 0.
    let pml4 = unsafe { current_cr3() };
    let pml4_index = ((virtual_address >> 39) & 0x1ff) as usize;
    let pdpt_index = ((virtual_address >> 30) & 0x1ff) as usize;
    let pd_index = ((virtual_address >> 21) & 0x1ff) as usize;
    let pt_index = ((virtual_address >> 12) & 0x1ff) as usize;

    let pdpt = entry_address(read_entry(pml4, pml4_index, hhdm_offset)?)?;
    let pdpt_entry = read_entry(pdpt, pdpt_index, hhdm_offset)?;
    if pdpt_entry & PRESENT == 0 {
        return None;
    }
    if pdpt_entry & HUGE_PAGE != 0 {
        let base = pdpt_entry & 0x000f_ffff_c000_0000;
        return Some(Translation {
            physical_address: base | (virtual_address & (PAGE_1G - 1)),
            page_size: PageSize::Size1GiB,
        });
    }

    let pd = entry_address(pdpt_entry)?;
    let pd_entry = read_entry(pd, pd_index, hhdm_offset)?;
    if pd_entry & PRESENT == 0 {
        return None;
    }
    if pd_entry & HUGE_PAGE != 0 {
        let base = pd_entry & 0x000f_ffff_ffe0_0000;
        return Some(Translation {
            physical_address: base | (virtual_address & (PAGE_2M - 1)),
            page_size: PageSize::Size2MiB,
        });
    }

    let pt = entry_address(pd_entry)?;
    let pt_entry = read_entry(pt, pt_index, hhdm_offset)?;
    let base = entry_address(pt_entry)?;
    Some(Translation {
        physical_address: base | (virtual_address & (PAGE_4K - 1)),
        page_size: PageSize::Size4KiB,
    })
}

/// Details of a new kernel address space with one private 4 KiB test mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AddressSpace {
    pub root_physical_address: u64,
    pub test_virtual_address: u64,
    pub test_physical_address: u64,
}

unsafe fn allocate_zeroed_table(
    allocator: &mut PhysicalFrameAllocator,
    hhdm_offset: u64,
) -> Option<(PhysFrame, *mut u64)> {
    let frame = allocator.allocate_frame()?;
    if frame.start_address & !ADDRESS_MASK != 0 {
        return None;
    }

    let last = frame.start_address.checked_add(PAGE_4K - 1)?;
    let virtual_start = physical_to_virtual(frame.start_address, hhdm_offset)?;
    physical_to_virtual(last, hhdm_offset)?;

    let table = virtual_start.as_u64() as *mut u64;
    // SAFETY: The allocator reserved this usable physical frame, and its first and
    // last bytes are addressable through Limine's direct map.
    unsafe { ptr::write_bytes(table, 0, 512) };

    Some((frame, table))
}

/// Clones the active kernel PML4, adds a private 4 KiB mapping, and activates the clone.
///
/// The new root shares existing lower-level kernel mappings with the old root. It is
/// an early kernel address-space foundation, not yet an isolated user-process space.
///
/// # Safety
/// Must run in ring 0 with a valid Limine HHDM and four-level paging enabled. Call only
/// during single-core early startup, before other CPUs or concurrent page-table writers
/// exist. The function changes CR3 and leaves the new root active on success.
pub unsafe fn create_kernel_address_space(
    allocator: &mut PhysicalFrameAllocator,
    hhdm_offset: u64,
) -> Option<AddressSpace> {
    let cr4: u64;
    // SAFETY: The caller guarantees ring 0 execution.
    unsafe {
        core::arch::asm!(
            "mov {}, cr4",
            out(reg) cr4,
            options(nomem, nostack, preserves_flags)
        );
    }
    const CR4_LA57: u64 = 1 << 12;
    if cr4 & CR4_LA57 != 0 {
        return None;
    }

    // SAFETY: This function is called from ring 0 during early startup.
    let original_root = unsafe { current_cr3() };
    let original_root_virtual = physical_to_virtual(original_root, hhdm_offset)?;
    let original_root_pointer = original_root_virtual.as_u64() as *const u64;

    // SAFETY: The physical frame allocator returns a fresh usable page.
    let (new_root, new_root_pointer) = unsafe { allocate_zeroed_table(allocator, hhdm_offset)? };

    // SAFETY: Both pointers address distinct, valid 4 KiB page-table pages.
    unsafe { ptr::copy_nonoverlapping(original_root_pointer, new_root_pointer, 512) };

    let mut free_slot = None;
    for index in 257..511 {
        // SAFETY: index is within the 512-entry PML4 page.
        if unsafe { ptr::read_volatile(new_root_pointer.add(index)) } & PRESENT == 0 {
            free_slot = Some(index);
            break;
        }
    }
    let slot = free_slot?;

    let test_virtual_address = 0xffff_0000_0000_0000 | ((slot as u64) << 39) | PAGE_4K;

    let data_frame = allocator.allocate_frame()?;
    if data_frame.start_address & !ADDRESS_MASK != 0 {
        return None;
    }
    let data_last = data_frame.start_address.checked_add(PAGE_4K - 1)?;
    let data_virtual = physical_to_virtual(data_frame.start_address, hhdm_offset)?;
    physical_to_virtual(data_last, hhdm_offset)?;
    let data_pointer = data_virtual.as_u64() as *mut u8;
    // SAFETY: The frame is reserved from usable memory and fully addressable via HHDM.
    unsafe { ptr::write_bytes(data_pointer, 0, PAGE_4K as usize) };

    // SAFETY: Each table is a fresh, reserved, HHDM-addressable physical frame.
    let (pdpt_frame, pdpt) = unsafe { allocate_zeroed_table(allocator, hhdm_offset)? };
    let (pd_frame, pd) = unsafe { allocate_zeroed_table(allocator, hhdm_offset)? };
    let (pt_frame, pt) = unsafe { allocate_zeroed_table(allocator, hhdm_offset)? };

    let pdpt_index = ((test_virtual_address >> 30) & 0x1ff) as usize;
    let pd_index = ((test_virtual_address >> 21) & 0x1ff) as usize;
    let pt_index = ((test_virtual_address >> 12) & 0x1ff) as usize;

    // SAFETY: The entries are within freshly allocated, zeroed page-table pages.
    unsafe {
        ptr::write_volatile(
            new_root_pointer.add(slot),
            pdpt_frame.start_address | PRESENT | (1 << 1),
        );
        ptr::write_volatile(
            pdpt.add(pdpt_index),
            pd_frame.start_address | PRESENT | (1 << 1),
        );
        ptr::write_volatile(
            pd.add(pd_index),
            pt_frame.start_address | PRESENT | (1 << 1),
        );
        ptr::write_volatile(
            pt.add(pt_index),
            data_frame.start_address | PRESENT | (1 << 1),
        );
    }

    compiler_fence(Ordering::SeqCst);
    // SAFETY: The cloned PML4 retains the current kernel and HHDM mappings, and all
    // new page-table pages were populated before this CR3 switch.
    unsafe {
        core::arch::asm!(
            "mov cr3, {}",
            in(reg) new_root.start_address,
            options(nostack, preserves_flags)
        );
    }

    const TEST_PATTERN: u64 = 0x5345_5354_494e_4101;
    let test_pointer = test_virtual_address as *mut u64;
    let direct_pointer = data_virtual.as_u64() as *const u64;

    // SAFETY: The new PTE maps the reserved data frame at test_virtual_address.
    unsafe { ptr::write_volatile(test_pointer, TEST_PATTERN) };
    // SAFETY: Both addresses alias the same physical page through the new mapping
    // and the existing HHDM mapping.
    let mapped_value = unsafe { ptr::read_volatile(test_pointer) };
    let direct_value = unsafe { ptr::read_volatile(direct_pointer) };

    if mapped_value != TEST_PATTERN || direct_value != TEST_PATTERN {
        // SAFETY: Restore the original root if the mapping self-test fails.
        unsafe {
            core::arch::asm!(
                "mov cr3, {}",
                in(reg) original_root,
                options(nostack, preserves_flags)
            );
        }
        return None;
    }

    Some(AddressSpace {
        root_physical_address: new_root.start_address,
        test_virtual_address,
        test_physical_address: data_frame.start_address,
    })
}
