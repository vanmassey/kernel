use super::frame::physical_to_virtual;

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
pub unsafe fn translate_current(
    virtual_address: u64,
    hhdm_offset: u64,
) -> Option<Translation> {
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
