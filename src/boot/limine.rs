use core::ptr;

const LIMINE_COMMON_MAGIC_0: u64 = 0xc7b1_dd30_df4c_8b88;
const LIMINE_COMMON_MAGIC_1: u64 = 0x0a82_e883_a194_f07b;
const LIMINE_MEMMAP_REQUEST_ID_0: u64 = 0x67cf_3d9d_378a_806f;
const LIMINE_MEMMAP_REQUEST_ID_1: u64 = 0xe304_acdf_c50c_3c62;

const MEMMAP_USABLE: u64 = 0;

const LIMINE_HHDM_REQUEST_ID_0: u64 = 0x48dc_f1cb_8ad2_b852;
const LIMINE_HHDM_REQUEST_ID_1: u64 = 0x6398_4e95_9a98_244b;

#[repr(C)]
struct HhdmResponse {
    revision: u64,
    offset: u64,
}

#[repr(C)]
struct HhdmRequest {
    id: [u64; 4],
    revision: u64,
    response: *mut HhdmResponse,
}

#[used]
#[link_section = ".limine_requests"]
static mut HHDM_REQUEST: HhdmRequest = HhdmRequest {
    id: [
        LIMINE_COMMON_MAGIC_0,
        LIMINE_COMMON_MAGIC_1,
        LIMINE_HHDM_REQUEST_ID_0,
        LIMINE_HHDM_REQUEST_ID_1,
    ],
    revision: 0,
    response: ptr::null_mut(),
};

/// Returns the higher-half direct-map offset supplied by Limine.
///
/// # Safety
/// Must only be called after Limine has transferred control to the kernel.
/// The response pointer must remain valid for the duration of this call.
pub unsafe fn hhdm_offset() -> Option<u64> {
    // SAFETY: Limine writes this response pointer before entering the kernel.
    let response = unsafe { ptr::read_volatile(ptr::addr_of!(HHDM_REQUEST.response)) };
    if response.is_null() {
        return None;
    }

    // SAFETY: The response pointer is non-null and owned by the bootloader.
    Some(unsafe { ptr::read_volatile(ptr::addr_of!((*response).offset)) })
}

const MAX_MEMORY_MAP_ENTRIES: u64 = 4096;

#[repr(C)]
pub(crate) struct MemoryMapEntry {
    pub(crate) base: u64,
    pub(crate) length: u64,
    pub(crate) entry_type: u64,
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

pub(crate) struct MemoryMapRegion {
    pub base: u64,
    pub length: u64,
    pub entry_type: u64,
}

pub(crate) struct MemoryMapEntries {
    entries: *const *const MemoryMapEntry,
    count: u64,
    index: u64,
}

impl Iterator for MemoryMapEntries {
    type Item = MemoryMapRegion;

    fn next(&mut self) -> Option<Self::Item> {
        while self.index < self.count {
            let index = self.index;
            self.index += 1;

            // SAFETY: The constructor validates the Limine array and bounds the entry count.
            let entry = unsafe { ptr::read_volatile(self.entries.add(index as usize)) };
            if entry.is_null() {
                return None;
            }

            // SAFETY: Each non-null pointer is a Limine memory-map entry.
            let base = unsafe { ptr::read_volatile(ptr::addr_of!((*entry).base)) };
            let length = unsafe { ptr::read_volatile(ptr::addr_of!((*entry).length)) };
            let entry_type = unsafe { ptr::read_volatile(ptr::addr_of!((*entry).entry_type)) };

            return Some(MemoryMapRegion {
                base,
                length,
                entry_type,
            });
        }

        None
    }
}

/// Creates an iterator over the memory map supplied by Limine.
///
/// # Safety
/// Call only after Limine has transferred control to the kernel. The bootloader-owned
/// response and entry array must remain mapped and valid while the iterator is used.
pub(crate) unsafe fn memory_map_entries() -> Option<MemoryMapEntries> {
    // SAFETY: Limine writes the response pointer before entering the kernel.
    let response = unsafe { ptr::read_volatile(ptr::addr_of!(MEMORY_MAP_REQUEST.response)) };
    if response.is_null() {
        return None;
    }

    // SAFETY: The response pointer is non-null and owned by the bootloader.
    let count = unsafe { ptr::read_volatile(ptr::addr_of!((*response).entry_count)) };
    let entries = unsafe { ptr::read_volatile(ptr::addr_of!((*response).entries)) };

    if count > MAX_MEMORY_MAP_ENTRIES || (count != 0 && entries.is_null()) {
        return None;
    }

    Some(MemoryMapEntries {
        entries,
        count,
        index: 0,
    })
}

const LIMINE_FRAMEBUFFER_REQUEST_ID_0: u64 = 0x9d58_27dc_d881_dd75;
const LIMINE_FRAMEBUFFER_REQUEST_ID_1: u64 = 0xa314_8604_f6fa_b11b;

#[repr(C)]
pub(crate) struct LimineFramebuffer {
    pub address: *mut u8,
    pub width: u64,
    pub height: u64,
    pub pitch: u64,
    pub bpp: u16,
    pub memory_model: u8,
    pub red_mask_size: u8,
    pub red_mask_shift: u8,
    pub green_mask_size: u8,
    pub green_mask_shift: u8,
    pub blue_mask_size: u8,
    pub blue_mask_shift: u8,
    unused: [u8; 7],
    edid_size: u64,
    edid: *mut core::ffi::c_void,
    mode_count: u64,
    modes: *const *mut core::ffi::c_void,
}

#[repr(C)]
struct FramebufferResponse {
    revision: u64,
    framebuffer_count: u64,
    framebuffers: *const *mut LimineFramebuffer,
}

#[repr(C)]
struct FramebufferRequest {
    id: [u64; 4],
    revision: u64,
    response: *mut FramebufferResponse,
}

#[used]
#[link_section = ".limine_requests"]
static mut FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest {
    id: [
        LIMINE_COMMON_MAGIC_0,
        LIMINE_COMMON_MAGIC_1,
        LIMINE_FRAMEBUFFER_REQUEST_ID_0,
        LIMINE_FRAMEBUFFER_REQUEST_ID_1,
    ],
    revision: 0,
    response: ptr::null_mut(),
};

/// Returns the first framebuffer described by Limine.
///
/// # Safety
/// Must only be called after Limine has transferred control to the kernel.
/// The response and framebuffer structures must remain mapped and valid.
pub(crate) unsafe fn framebuffer() -> Option<FramebufferInfo> {
    let response = unsafe { ptr::read_volatile(ptr::addr_of!(FRAMEBUFFER_REQUEST.response)) };
    if response.is_null() {
        return None;
    }

    let count = unsafe { ptr::read_volatile(ptr::addr_of!((*response).framebuffer_count)) };
    let framebuffers = unsafe { ptr::read_volatile(ptr::addr_of!((*response).framebuffers)) };
    if count == 0 || count > 16 || framebuffers.is_null() {
        return None;
    }

    let framebuffer = unsafe { ptr::read_volatile(framebuffers) };
    if framebuffer.is_null() {
        return None;
    }

    Some(FramebufferInfo {
        address: unsafe { ptr::read_volatile(ptr::addr_of!((*framebuffer).address)) },
        width: unsafe { ptr::read_volatile(ptr::addr_of!((*framebuffer).width)) },
        height: unsafe { ptr::read_volatile(ptr::addr_of!((*framebuffer).height)) },
        pitch: unsafe { ptr::read_volatile(ptr::addr_of!((*framebuffer).pitch)) },
        bpp: unsafe { ptr::read_volatile(ptr::addr_of!((*framebuffer).bpp)) },
        memory_model: unsafe { ptr::read_volatile(ptr::addr_of!((*framebuffer).memory_model)) },
        red_mask_size: unsafe { ptr::read_volatile(ptr::addr_of!((*framebuffer).red_mask_size)) },
        red_mask_shift: unsafe { ptr::read_volatile(ptr::addr_of!((*framebuffer).red_mask_shift)) },
        green_mask_size: unsafe {
            ptr::read_volatile(ptr::addr_of!((*framebuffer).green_mask_size))
        },
        green_mask_shift: unsafe {
            ptr::read_volatile(ptr::addr_of!((*framebuffer).green_mask_shift))
        },
        blue_mask_size: unsafe { ptr::read_volatile(ptr::addr_of!((*framebuffer).blue_mask_size)) },
        blue_mask_shift: unsafe {
            ptr::read_volatile(ptr::addr_of!((*framebuffer).blue_mask_shift))
        },
    })
}

#[derive(Clone, Copy)]
pub(crate) struct FramebufferInfo {
    pub address: *mut u8,
    pub width: u64,
    pub height: u64,
    pub pitch: u64,
    pub bpp: u16,
    pub memory_model: u8,
    pub red_mask_size: u8,
    pub red_mask_shift: u8,
    pub green_mask_size: u8,
    pub green_mask_shift: u8,
    pub blue_mask_size: u8,
    pub blue_mask_shift: u8,
}
