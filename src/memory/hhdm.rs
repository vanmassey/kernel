/// A checked virtual address in the x86_64 address space.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VirtAddr(u64);

impl VirtAddr {
    pub const fn as_u64(self) -> u64 { self.0 }
}

/// Convert a physical address through Limine's higher-half direct map.
pub fn physical_to_virtual(physical_address: u64, direct_map_offset: u64) -> Option<VirtAddr> {
    let address = physical_address.checked_add(direct_map_offset)?;
    let upper = address >> 48;
    let sign = (address >> 47) & 1;
    if (sign == 0 && upper == 0) || (sign == 1 && upper == 0xffff) {
        Some(VirtAddr(address))
    } else {
        None
    }
}
