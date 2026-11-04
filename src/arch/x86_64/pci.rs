//! Legacy PCI configuration-space access and PCI-to-PCI bridge enumeration.
//!
//! This implementation uses PCI Configuration Mechanism #1 (CF8/CFC). It
//! enumerates conventional PCI configuration space; PCIe extended configuration
//! space and ECAM are intentionally left for a later ACPI/MPCI-aware stage.

const CONFIG_ADDRESS: u16 = 0x0cf8;
const CONFIG_DATA: u16 = 0x0cfc;
const VENDOR_ID_ABSENT: u16 = 0xffff;
const CLASS_BRIDGE: u8 = 0x06;
const SUBCLASS_PCI_BRIDGE: u8 = 0x04;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Device {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub revision_id: u8,
    pub programming_interface: u8,
    pub subclass: u8,
    pub class_code: u8,
    pub header_type: u8,
}

impl Device {
    pub const fn is_multifunction(self) -> bool {
        self.header_type & 0x80 != 0
    }

    pub const fn is_pci_bridge(self) -> bool {
        self.class_code == CLASS_BRIDGE && self.subclass == SUBCLASS_PCI_BRIDGE
    }
}

/// Enumerate devices reachable through conventional PCI buses.
///
/// The callback is invoked once per present function. PCI-to-PCI bridges are
/// followed using their secondary-bus register, with a visited-bus bitmap
/// preventing loops. The return value is the number of discovered functions.
pub fn enumerate(mut visit: impl FnMut(Device)) -> usize {
    let mut pending = [0_u8; 256];
    let mut pending_len = 1_usize;
    let mut next_pending = 0_usize;
    let mut visited = [0_u64; 4];
    pending[0] = 0;
    visited[0] |= 1;
    let mut count = 0;

    while next_pending < pending_len {
        let bus = pending[next_pending];
        next_pending += 1;

        for device in 0_u8..32 {
            let vendor0 = read_u16(bus, device, 0, 0x00);
            if vendor0 == VENDOR_ID_ABSENT {
                continue;
            }

            let header0 = read_u8(bus, device, 0, 0x0e);
            let function_limit = if header0 & 0x80 != 0 { 8 } else { 1 };

            for function in 0_u8..function_limit {
                let vendor_id = if function == 0 {
                    vendor0
                } else {
                    read_u16(bus, device, function, 0x00)
                };
                if vendor_id == VENDOR_ID_ABSENT {
                    continue;
                }

                let identity = read_u32(bus, device, function, 0x00);
                let class = read_u32(bus, device, function, 0x08);
                let header_type = read_u8(bus, device, function, 0x0e);
                let found = Device {
                    bus,
                    device,
                    function,
                    vendor_id,
                    device_id: (identity >> 16) as u16,
                    revision_id: class as u8,
                    programming_interface: (class >> 8) as u8,
                    subclass: (class >> 16) as u8,
                    class_code: (class >> 24) as u8,
                    header_type,
                };

                count += 1;
                visit(found);

                // Type-1 headers are PCI-to-PCI bridges. The secondary bus
                // number is at offset 0x19 in the bridge's standard header.
                if found.is_pci_bridge() && header_type & 0x7f == 1 {
                    let secondary = read_u8(bus, device, function, 0x19);
                    if secondary != 0 && pending_len < pending.len() {
                        let secondary_mask = 1_u64 << (secondary as usize % 64);
                        let secondary_slot = &mut visited[secondary as usize / 64];
                        if *secondary_slot & secondary_mask == 0 {
                            *secondary_slot |= secondary_mask;
                            pending[pending_len] = secondary;
                            pending_len += 1;
                        }
                    }
                }
            }
        }
    }

    count
}

/// Read a 32-bit value from a function's conventional PCI configuration space.
pub fn read_u32(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    let address = config_address(bus, device, function, offset);
    // SAFETY: CF8/CFC are the x86 PCI Configuration Mechanism #1 ports.
    // The caller executes in ring 0 and supplies a valid BDF and aligned offset.
    unsafe {
        outl(CONFIG_ADDRESS, address);
        inl(CONFIG_DATA)
    }
}

/// Read a 16-bit value from a function's conventional PCI configuration space.
pub fn read_u16(bus: u8, device: u8, function: u8, offset: u8) -> u16 {
    let value = read_u32(bus, device, function, offset & !0x03);
    (value >> ((offset & 0x02) * 8)) as u16
}

/// Read an 8-bit value from a function's conventional PCI configuration space.
pub fn read_u8(bus: u8, device: u8, function: u8, offset: u8) -> u8 {
    let value = read_u32(bus, device, function, offset & !0x03);
    (value >> ((offset & 0x03) * 8)) as u8
}

const fn config_address(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    0x8000_0000
        | ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((function as u32) << 8)
        | ((offset as u32) & 0xfc)
}

unsafe fn outl(port: u16, value: u32) {
    // SAFETY: The caller uses the standard PCI configuration address port.
    unsafe {
        core::arch::asm!(
            "out dx, eax",
            in("dx") port,
            in("eax") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

unsafe fn inl(port: u16) -> u32 {
    let value: u32;
    // SAFETY: The caller uses the standard PCI configuration data port.
    unsafe {
        core::arch::asm!(
            "in eax, dx",
            in("dx") port,
            out("eax") value,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}

#[cfg(test)]
mod tests {
    use super::config_address;

    #[test]
    fn config_address_encodes_bdf_and_aligned_offset() {
        assert_eq!(config_address(0, 0, 0, 0), 0x8000_0000);
        assert_eq!(config_address(2, 3, 1, 0x0b), 0x8002_1908);
        assert_eq!(config_address(255, 31, 7, 0xfc), 0x80ff_ffff);
    }
}
