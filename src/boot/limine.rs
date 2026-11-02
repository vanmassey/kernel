use core::ptr;

const LIMINE_COMMON_MAGIC_0: u64 = 0xc7b1_dd30_df4c_8b88;
const LIMINE_COMMON_MAGIC_1: u64 = 0x0a82_e883_a194_f07b;
const LIMINE_MEMMAP_REQUEST_ID_0: u64 = 0x67cf_3d9d_378a_806f;
const LIMINE_MEMMAP_REQUEST_ID_1: u64 = 0xe304_acdf_c50c_3c62;

const MEMMAP_USABLE: u64 = 0;
const MAX_MEMORY_MAP_ENTRIES: u64 = 4096;

#[repr(C)]
struct MemoryMapEntry {
    base: u64,
    length: u64,
    entry_type: u64,
}

#[repr(C)]
struct MemoryMapResponse {
    revision: u64,
    entry_count: u64,
    entries: *const *const MemoryMapEntry,
}

#[repr(C)]
struct MemoryMapRequest {
    id: [u64; 4],
    revision: u64,
    response: *mut MemoryMapResponse,
}

#[used]
#[link_section = ".limine_requests"]
static mut MEMORY_MAP_REQUEST: MemoryMapRequest = MemoryMapRequest {
    id: [
        LIMINE_COMMON_MAGIC_0,
        LIMINE_COMMON_MAGIC_1,
        LIMINE_MEMMAP_REQUEST_ID_0,
        LIMINE_MEMMAP_REQUEST_ID_1,
    ],
    revision: 0,
    response: ptr::null_mut(),
};

pub struct MemoryMapSummary {
    pub entry_count: u64,
    pub usable_region_count: u64,
    pub usable_bytes: u64,
}

/// Reads Limine's memory map response and totals the usable physical regions.
///
/// # Safety
/// Must only be called after Limine has transferred control to the kernel.
/// The response and entry pointers must remain valid for the duration of this call.
pub unsafe fn memory_map_summary() -> Option<MemoryMapSummary> {
    // SAFETY: The bootloader writes this response pointer before entering the kernel.
    let response = unsafe { ptr::read_volatile(ptr::addr_of!(MEMORY_MAP_REQUEST.response)) };
    if response.is_null() {
        return None;
    }

    // SAFETY: A non-null response pointer is supplied by the bootloader.
    let entry_count = unsafe { ptr::read_volatile(ptr::addr_of!((*response).entry_count)) };
    let entries = unsafe { ptr::read_volatile(ptr::addr_of!((*response).entries)) };

    if entry_count > MAX_MEMORY_MAP_ENTRIES || (entry_count != 0 && entries.is_null()) {
        return None;
    }

    let mut usable_region_count = 0;
    let mut usable_bytes = 0_u64;

    for index in 0..entry_count {
        // SAFETY: The response advertises entry_count pointers in the entries array.
        let entry_ptr = unsafe { ptr::read_volatile(entries.add(index as usize)) };
        if entry_ptr.is_null() {
            return None;
        }

        // SAFETY: Each non-null entry pointer refers to a Limine memory-map entry.
        let entry_type = unsafe { ptr::read_volatile(ptr::addr_of!((*entry_ptr).entry_type)) };
        if entry_type == MEMMAP_USABLE {
            let length = unsafe { ptr::read_volatile(ptr::addr_of!((*entry_ptr).length)) };
            usable_region_count += 1;
            usable_bytes = usable_bytes.saturating_add(length);
        }
    }

    Some(MemoryMapSummary {
        entry_count,
        usable_region_count,
        usable_bytes,
    })
}
