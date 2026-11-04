//! Allocation-free parsing of Ethernet/IPv4 ARP packets.

use super::ethernet::MacAddress;

/// The fixed size of an Ethernet/IPv4 ARP packet, in bytes.
pub const PACKET_LEN: usize = 28;

/// ARP operation codes supported by this parser.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operation {
    Request,
    Reply,
}

/// A four-byte IPv4 protocol address.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ipv4Address(pub [u8; 4]);

/// A parsed Ethernet/IPv4 ARP packet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArpPacket {
    pub operation: Operation,
    pub sender_hardware_address: MacAddress,
    pub sender_protocol_address: Ipv4Address,
    pub target_hardware_address: MacAddress,
    pub target_protocol_address: Ipv4Address,
}

/// Errors returned when an input buffer is not a supported ARP packet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseError {
    TruncatedPacket,
    UnsupportedHardwareType,
    UnsupportedProtocolType,
    InvalidAddressLengths,
    UnsupportedOperation,
}

/// Parse an Ethernet/IPv4 ARP packet from a byte buffer.
///
/// Trailing Ethernet padding is permitted and ignored. The parser does not
/// allocate or copy.
pub fn parse(packet: &[u8]) -> Result<ArpPacket, ParseError> {
    if packet.len() < PACKET_LEN {
        return Err(ParseError::TruncatedPacket);
    }

    if u16::from_be_bytes([packet[0], packet[1]]) != 1 {
        return Err(ParseError::UnsupportedHardwareType);
    }

    if u16::from_be_bytes([packet[2], packet[3]]) != 0x0800 {
        return Err(ParseError::UnsupportedProtocolType);
    }

    if packet[4] != 6 || packet[5] != 4 {
        return Err(ParseError::InvalidAddressLengths);
    }

    let operation = match u16::from_be_bytes([packet[6], packet[7]]) {
        1 => Operation::Request,
        2 => Operation::Reply,
        _ => return Err(ParseError::UnsupportedOperation),
    };

    Ok(ArpPacket {
        operation,
        sender_hardware_address: MacAddress([
            packet[8], packet[9], packet[10], packet[11], packet[12], packet[13],
        ]),
        sender_protocol_address: Ipv4Address([
            packet[14], packet[15], packet[16], packet[17],
        ]),
        target_hardware_address: MacAddress([
            packet[18], packet[19], packet[20], packet[21], packet[22], packet[23],
        ]),
        target_protocol_address: Ipv4Address([
            packet[24], packet[25], packet[26], packet[27],
        ]),
    })
}

#[cfg(test)]
mod tests {
    use super::{parse, Ipv4Address, Operation, ParseError, PACKET_LEN};
    use crate::net::ethernet::MacAddress;

    fn request() -> [u8; PACKET_LEN] {
        [
            0x00, 0x01, // Ethernet
            0x08, 0x00, // IPv4
            0x06, 0x04, // address lengths
            0x00, 0x01, // request
            0x02, 0x00, 0x00, 0x00, 0x00, 0x01, // sender MAC
            192, 168, 1, 10, // sender IPv4
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // target MAC
            192, 168, 1, 1, // target IPv4
        ]
    }

    #[test]
    fn parses_request_fields() {
        let parsed = parse(&request()).expect("valid ARP request");
        assert_eq!(parsed.operation, Operation::Request);
        assert_eq!(
            parsed.sender_hardware_address,
            MacAddress([2, 0, 0, 0, 0, 1])
        );
        assert_eq!(
            parsed.sender_protocol_address,
            Ipv4Address([192, 168, 1, 10])
        );
        assert_eq!(parsed.target_hardware_address, MacAddress([0; 6]));
        assert_eq!(
            parsed.target_protocol_address,
            Ipv4Address([192, 168, 1, 1])
        );
    }

    #[test]
    fn parses_reply() {
        let mut bytes = request();
        bytes[7] = 2;
        assert_eq!(parse(&bytes).unwrap().operation, Operation::Reply);
    }

    #[test]
    fn accepts_trailing_ethernet_padding() {
        let mut bytes = [0; PACKET_LEN + 18];
        bytes[..PACKET_LEN].copy_from_slice(&request());
        assert_eq!(parse(&bytes).unwrap().operation, Operation::Request);
    }

    #[test]
    fn rejects_truncated_packet() {
        assert_eq!(
            parse(&[0; PACKET_LEN - 1]),
            Err(ParseError::TruncatedPacket)
        );
    }

    #[test]
    fn rejects_unsupported_hardware_protocol_lengths_and_operation() {
        let mut bytes = request();
        bytes[1] = 2;
        assert_eq!(parse(&bytes), Err(ParseError::UnsupportedHardwareType));

        let mut bytes = request();
        bytes[3] = 1;
        assert_eq!(parse(&bytes), Err(ParseError::UnsupportedProtocolType));

        let mut bytes = request();
        bytes[4] = 8;
        assert_eq!(parse(&bytes), Err(ParseError::InvalidAddressLengths));

        let mut bytes = request();
        bytes[7] = 3;
        assert_eq!(parse(&bytes), Err(ParseError::UnsupportedOperation));
    }
}
