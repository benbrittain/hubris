#![no_std]

use derive_idol_err::IdolError;
use serde::{Deserialize, Serialize};
use userlib::*;
use zerocopy::{AsBytes, FromBytes};

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct UdpMetadata {
    pub addr: Ipv6Address,
    pub port: u16,
    pub payload_len: u32,
}

impl From<UdpMetadata> for smoltcp::wire::IpEndpoint {
    fn from(m: UdpMetadata) -> Self {
        Self {
            addr: m.addr.into(),
            port: m.port,
        }
    }
}

impl From<Ipv6Address> for smoltcp::wire::IpAddress {
    fn from(a: Ipv6Address) -> Self {
        Self::Ipv6(a.into())
    }
}

impl From<[u8; 16]> for Ipv6Address {
    fn from(a: [u8; 16]) -> Self {
        Ipv6Address(a)
    }
}

impl TryFrom<smoltcp::wire::IpAddress> for Ipv6Address {
    type Error = ();

    // We implement a TryFrom due to the socket.recv() api response, but we should
    // *NEVER* get anything besides a Ipv6 addr.
    fn try_from(a: smoltcp::wire::IpAddress) -> Result<Self, Self::Error> {
        use smoltcp::wire::IpAddress;

        match a {
            IpAddress::Ipv6(a) => Ok(a.into()),
            _ => Err(()),
        }
    }
}

impl core::fmt::Display for Ipv6Address {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            "{:02x}-{:02x}-{:02x}-{:02x}-{:02x}-{:02x}-{:02x}-{:02x}",
            self.0[0],
            self.0[1],
            self.0[2],
            self.0[3],
            self.0[4],
            self.0[5],
            self.0[6],
            self.0[7]
        )
    }
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, FromBytes, AsBytes)]
#[repr(C)]
#[serde(transparent)]
/// An extended 802.15.4 address.
pub struct Ieee802154Address(pub [u8; 8]);

impl From<smoltcp::wire::Ieee802154Address> for Ieee802154Address {
    fn from(a: smoltcp::wire::Ieee802154Address) -> Self {
        match a {
            smoltcp::wire::Ieee802154Address::Extended(e) => Self(e),
            _ => panic!("This is not an extended address!"),
        }
    }
}

impl From<Ieee802154Address> for smoltcp::wire::Ieee802154Address {
    fn from(a: Ieee802154Address) -> Self {
        smoltcp::wire::Ieee802154Address::Extended(a.0)
    }
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, FromBytes, AsBytes)]
#[repr(C)]
#[serde(transparent)]
pub struct Ipv6Address(pub [u8; 16]);

impl From<smoltcp::wire::Ipv6Address> for Ipv6Address {
    fn from(a: smoltcp::wire::Ipv6Address) -> Self {
        Self(a.0)
    }
}

impl From<Ipv6Address> for smoltcp::wire::Ipv6Address {
    fn from(a: Ipv6Address) -> Self {
        Self(a.0)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, FromPrimitive, IdolError)]
#[repr(u32)]
pub enum AetherError {
    /// No Packets to recieve. Will not wake task until there is a packet
    QueueEmpty = 1,
    /// No space in the transmit buffer
    NoTransmitSlot,
    /// This socket is owned by a different task (check app.toml)
    WrongOwner,
    /// Unknown Error from smoltcp socket.
    Unknown,
}

include!(concat!(env!("OUT_DIR"), "/client_stub.rs"));
include!(concat!(env!("OUT_DIR"), "/aether_config.rs"));
