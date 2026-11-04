//! Allocation-free parsing of Ethernet II frames.

/// The length of an Ethernet II header, in bytes.
pub const HEADER_LEN: usize = 14;

/// A six-byte Ethernet hardware address.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MacAddress(pub [u8; 6]);

impl MacAddress {
    /// The broadcast destination address.
    pub const BROADCAST: Self = Self([0xff; 6]);

    /// Returns the address as six bytes.
    pub const fn as_bytes(&self) -> &[u8; 6] {
        &self.0
    }
}

/// The EtherType field from an Ethernet II header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EtherType(pub u16);

impl EtherType {
    pub const IPV4: Self = Self(0x0800);
    pub const ARP: Self = Self(0x0806);
    pub const IPV6: Self = Self(0x86dd);
}

/// A parsed Ethernet II frame borrowing its payload from the input buffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EthernetFrame<'a> {
    pub destination: MacAddress,
    pub source: MacAddress,
    pub ether_type: EtherType,
    pub payload: &'a [u8],
}

/// Errors returned when an input buffer is not a valid Ethernet II frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseError {
    /// The input does not contain a complete 14-byte Ethernet header.
    TruncatedHeader,
    /// The type/length field contains an IEEE 802.3 length, not an EtherType.
    NotEthernetII,
}

/// Parse one Ethernet II frame from a borrowed byte buffer.
///
/// The parser does not allocate or copy. The returned payload borrows directly
/// from `frame`. Any frame-check sequence bytes supplied by the caller remain
/// part of the payload; hardware drivers may strip them before calling this
/// function.
///
/// Ethernet II EtherTypes begin at 0x0600. Values below that boundary are
/// interpreted as IEEE 802.3 length fields and are rejected.
pub fn parse(frame: &[u8]) -> Result<EthernetFrame<'_>, ParseError> {
    if frame.len() < HEADER_LEN {
        return Err(ParseError::TruncatedHeader);
    }

    let destination = MacAddress([
        frame[0], frame[1], frame[2], frame[3], frame[4], frame[5],
    ]);
    let source = MacAddress([
        frame[6], frame[7], frame[8], frame[9], frame[10], frame[11],
    ]);
    let raw_type = u16::from_be_bytes([frame[12], frame[13]]);

    if raw_type < 0x0600 {
        return Err(ParseError::NotEthernetII);
    }

    Ok(EthernetFrame {
        destination,
        source,
        ether_type: EtherType(raw_type),
        payload: &frame[HEADER_LEN..],
    })
}

#[cfg(test)]
mod tests {
    use super::{parse, EtherType, MacAddress, ParseError, HEADER_LEN};

    #[test]
    fn parses_ethernet_ipv4_frame() {
        let frame = [
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, // destination
            0x02, 0x00, 0x00, 0x00, 0x00, 0x01, // source
            0x08, 0x00, // IPv4 EtherType
            0x45, 0x00, 0x00, 0x14, // payload
        ];

        let parsed = parse(&frame).expect("valid Ethernet II frame");
        assert_eq!(parsed.destination, MacAddress::BROADCAST);
        assert_eq!(parsed.source, MacAddress([2, 0, 0, 0, 0, 1]));
        assert_eq!(parsed.ether_type, EtherType::IPV4);
        assert_eq!(parsed.payload, &[0x45, 0x00, 0x00, 0x14]);
    }

    #[test]
    fn parses_minimum_header_with_empty_payload() {
        let frame = [0_u8; HEADER_LEN];
        let mut frame = frame;
        frame[12] = 0x86;
        frame[13] = 0xdd;

        let parsed = parse(&frame).expect("valid header");
        assert_eq!(parsed.ether_type, EtherType::IPV6);
        assert!(parsed.payload.is_empty());
    }

    #[test]
    fn rejects_truncated_header() {
        assert_eq!(parse(&[0_u8; HEADER_LEN - 1]), Err(ParseError::TruncatedHeader));
        assert_eq!(parse(&[]), Err(ParseError::TruncatedHeader));
    }

    #[test]
    fn rejects_ieee_802_3_length_field() {
        let mut frame = [0_u8; HEADER_LEN];
        frame[12] = 0x05;
        frame[13] = 0xdc; // 1500, an IEEE 802.3 length
        assert_eq!(parse(&frame), Err(ParseError::NotEthernetII));
    }

    #[test]
    fn payload_borrows_the_original_buffer() {
        let mut frame = [0_u8; HEADER_LEN + 3];
        frame[12] = 0x08;
        frame[13] = 0x06;
        frame[HEADER_LEN..].copy_from_slice(&[1, 2, 3]);

        let parsed = parse(&frame).expect("valid ARP frame");
        assert_eq!(parsed.payload.as_ptr(), frame[HEADER_LEN..].as_ptr());
        assert_eq!(parsed.payload, &[1, 2, 3]);
    }
}
