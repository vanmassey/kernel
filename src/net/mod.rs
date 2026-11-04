//! Networking protocols and packet handling.
//!
//! The initial implementation is deliberately hardware-independent. Protocol
//! parsers operate on borrowed byte slices; NIC drivers and packet queues can
//! be added as separate layers later.

pub mod arp;
pub mod arp_cache;
pub mod ethernet;
