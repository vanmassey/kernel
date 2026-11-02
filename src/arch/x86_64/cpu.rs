//! x86_64 CPU identification and feature discovery.

use core::arch::x86_64::__cpuid;

/// A small snapshot of CPU identity and selected architectural capabilities.
pub struct CpuInfo {
    pub vendor: [u8; 12],
    pub max_basic_leaf: u32,
    pub max_extended_leaf: u32,
    pub has_sse2: bool,
    pub has_apic: bool,
    pub has_x2apic: bool,
    pub has_nx: bool,
    pub has_1gib_pages: bool,
    pub has_syscall: bool,
    pub has_long_mode: bool,
}

/// Queries CPUID and returns the processor identity and selected feature bits.
///
/// # Safety
/// CPUID is available on all x86_64 processors. The intrinsic is used only
/// with architectural leaf values and does not access memory or device state.
pub fn detect() -> CpuInfo {
    // SAFETY: CPUID leaf 0 is supported on every x86_64 processor.
    let basic = unsafe { __cpuid(0) };
    let mut vendor = [0u8; 12];
    vendor[0..4].copy_from_slice(&basic.ebx.to_le_bytes());
    vendor[4..8].copy_from_slice(&basic.edx.to_le_bytes());
    vendor[8..12].copy_from_slice(&basic.ecx.to_le_bytes());

    // SAFETY: Query leaf 1 only when the processor advertises its availability.
    let leaf1 = if basic.eax >= 1 {
        unsafe { __cpuid(1) }
    } else {
        unsafe { __cpuid(0) }
    };

    // SAFETY: The extended maximum-leaf query is defined by the x86 CPUID interface.
    let extended = unsafe { __cpuid(0x8000_0000) };
    let max_extended_leaf = extended.eax;

    // SAFETY: Query the extended feature leaf only when it is advertised.
    let extended_features = if max_extended_leaf >= 0x8000_0001 {
        unsafe { __cpuid(0x8000_0001) }
    } else {
        // Leaf zero is a safe fallback; its feature bits are not interpreted below.
        unsafe { __cpuid(0) }
    };

    CpuInfo {
        vendor,
        max_basic_leaf: basic.eax,
        max_extended_leaf,
        has_sse2: leaf1.edx & (1 << 26) != 0,
        has_apic: leaf1.edx & (1 << 9) != 0,
        has_x2apic: leaf1.ecx & (1 << 21) != 0,
        has_nx: max_extended_leaf >= 0x8000_0001 && extended_features.edx & (1 << 20) != 0,
        has_1gib_pages: max_extended_leaf >= 0x8000_0001 && extended_features.edx & (1 << 26) != 0,
        has_syscall: max_extended_leaf >= 0x8000_0001 && extended_features.edx & (1 << 11) != 0,
        has_long_mode: max_extended_leaf >= 0x8000_0001 && extended_features.edx & (1 << 29) != 0,
    }
}
