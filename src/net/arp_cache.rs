//! Fixed-capacity, allocation-free ARP cache.

use super::arp::Ipv4Address;
use super::ethernet::MacAddress;

/// Maximum number of IPv4-to-MAC mappings retained by the cache.
pub const CACHE_CAPACITY: usize = 16;

/// Default lifetime for a learned mapping (five minutes).
pub const ENTRY_TTL_MS: u64 = 5 * 60 * 1_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Entry {
    protocol_address: Ipv4Address,
    hardware_address: MacAddress,
    learned_at_ms: u64,
}

/// A fixed-size ARP cache.
///
/// Time is supplied by the caller in monotonic milliseconds. This keeps the
/// cache independent of the kernel timer implementation and avoids allocation.
pub struct ArpCache {
    entries: [Option<Entry>; CACHE_CAPACITY],
    next_replacement: usize,
}

impl ArpCache {
    /// Create an empty cache.
    pub const fn new() -> Self {
        Self {
            entries: [None; CACHE_CAPACITY],
            next_replacement: 0,
        }
    }

    /// Return the hardware address for an IPv4 address if a live entry exists.
    ///
    /// Expired entries are removed during lookup.
    pub fn lookup(&mut self, protocol_address: Ipv4Address, now_ms: u64) -> Option<MacAddress> {
        for entry in &mut self.entries {
            if let Some(candidate) = *entry {
                if Self::is_expired(candidate, now_ms) {
                    *entry = None;
                } else if candidate.protocol_address == protocol_address {
                    return Some(candidate.hardware_address);
                }
            }
        }

        None
    }

    /// Insert or refresh an IPv4-to-MAC mapping.
    ///
    /// Existing mappings for the same IPv4 address are updated in place. An
    /// unused or expired slot is preferred; if the cache is full, entries are
    /// replaced in round-robin order.
    pub fn insert(
        &mut self,
        protocol_address: Ipv4Address,
        hardware_address: MacAddress,
        now_ms: u64,
    ) {
        let new_entry = Some(Entry {
            protocol_address,
            hardware_address,
            learned_at_ms: now_ms,
        });

        // Refresh an existing mapping before considering another slot.
        for entry in &mut self.entries {
            if let Some(candidate) = *entry {
                if candidate.protocol_address == protocol_address {
                    *entry = new_entry;
                    return;
                }
            }
        }

        // Reuse an empty or expired slot before evicting a live mapping.
        for entry in &mut self.entries {
            if entry.is_none() || entry.is_some_and(|candidate| Self::is_expired(candidate, now_ms))
            {
                *entry = new_entry;
                return;
            }
        }

        self.entries[self.next_replacement] = new_entry;
        self.next_replacement = (self.next_replacement + 1) % CACHE_CAPACITY;
    }

    /// Remove all expired mappings and return the number removed.
    pub fn remove_expired(&mut self, now_ms: u64) -> usize {
        let mut removed = 0;

        for entry in &mut self.entries {
            if entry.is_some_and(|candidate| Self::is_expired(candidate, now_ms)) {
                *entry = None;
                removed += 1;
            }
        }

        removed
    }

    fn is_expired(entry: Entry, now_ms: u64) -> bool {
        now_ms.wrapping_sub(entry.learned_at_ms) >= ENTRY_TTL_MS
    }
}

impl Default for ArpCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{ArpCache, CACHE_CAPACITY, ENTRY_TTL_MS};
    use crate::net::arp::Ipv4Address;
    use crate::net::ethernet::MacAddress;

    fn ip(last_octet: u8) -> Ipv4Address {
        Ipv4Address([192, 168, 1, last_octet])
    }

    fn mac(last_octet: u8) -> MacAddress {
        MacAddress([0x02, 0, 0, 0, 0, last_octet])
    }

    #[test]
    fn inserts_and_looks_up_mapping() {
        let mut cache = ArpCache::new();
        cache.insert(ip(10), mac(10), 1_000);

        assert_eq!(cache.lookup(ip(10), 1_001), Some(mac(10)));
        assert_eq!(cache.lookup(ip(11), 1_001), None);
    }

    #[test]
    fn refreshes_existing_mapping_without_duplicate() {
        let mut cache = ArpCache::new();
        cache.insert(ip(10), mac(10), 1_000);
        cache.insert(ip(10), mac(20), 2_000);

        assert_eq!(cache.lookup(ip(10), 2_001), Some(mac(20)));
    }

    #[test]
    fn expires_entries_during_lookup() {
        let mut cache = ArpCache::new();
        cache.insert(ip(10), mac(10), 1_000);

        assert_eq!(cache.lookup(ip(10), 1_000 + ENTRY_TTL_MS), None);
    }

    #[test]
    fn removes_expired_entries_and_reports_count() {
        let mut cache = ArpCache::new();
        cache.insert(ip(10), mac(10), 0);
        cache.insert(ip(11), mac(11), 1);

        assert_eq!(cache.remove_expired(ENTRY_TTL_MS + 1), 2);
        assert_eq!(cache.lookup(ip(10), ENTRY_TTL_MS + 1), None);
    }

    #[test]
    fn reuses_expired_slot_before_evicting_live_entries() {
        let mut cache = ArpCache::new();
        for octet in 1..=CACHE_CAPACITY as u8 {
            cache.insert(ip(octet), mac(octet), 0);
        }

        cache.insert(ip(100), mac(100), ENTRY_TTL_MS);
        assert_eq!(cache.lookup(ip(1), ENTRY_TTL_MS), None);
        assert_eq!(cache.lookup(ip(100), ENTRY_TTL_MS), Some(mac(100)));
        assert_eq!(cache.lookup(ip(2), ENTRY_TTL_MS), Some(mac(2)));
    }

    #[test]
    fn replaces_oldest_slot_in_round_robin_order_when_full() {
        let mut cache = ArpCache::new();
        for octet in 1..=CACHE_CAPACITY as u8 {
            cache.insert(ip(octet), mac(octet), 0);
        }

        cache.insert(ip(100), mac(100), 1);
        assert_eq!(cache.lookup(ip(1), 2), None);
        assert_eq!(cache.lookup(ip(2), 2), Some(mac(2)));

        cache.insert(ip(101), mac(101), 3);
        assert_eq!(cache.lookup(ip(2), 4), None);
        assert_eq!(cache.lookup(ip(3), 4), Some(mac(3)));
    }

    #[test]
    fn default_creates_an_empty_cache() {
        let mut cache = ArpCache::default();
        assert_eq!(cache.lookup(ip(10), 0), None);
    }
}
