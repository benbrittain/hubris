#![no_std]

use derive_idol_err::IdolError;
use serde::{Deserialize, Serialize};
use userlib::*;
use zerocopy::{FromBytes, AsBytes};

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct UdpMetadata {
    pub addr: Address,
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

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum Address {
    Ipv6(Ipv6Address),
}

impl From<Address> for smoltcp::wire::IpAddress {
    fn from(a: Address) -> Self {
        match a {
            Address::Ipv6(a) => Self::Ipv6(a.into()),
        }
    }
}

impl TryFrom<smoltcp::wire::IpAddress> for Address {
    type Error = AddressUnspecified;

    fn try_from(a: smoltcp::wire::IpAddress) -> Result<Self, Self::Error> {
        use smoltcp::wire::IpAddress;

        match a {
            IpAddress::Ipv6(a) => Ok(Self::Ipv6(a.into())),
            _ => Err(AddressUnspecified),
        }
    }
}

pub struct AddressUnspecified;

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
    NoTransmitSlot = 2,
    /// This socket is owned by a different task (check app.toml)
    WrongOwner = 3,
    /// Unknown Error from smoltcp socket.
    Unknown = 4,
}

include!(concat!(env!("OUT_DIR"), "/client_stub.rs"));
include!(concat!(env!("OUT_DIR"), "/aether_config.rs"));
