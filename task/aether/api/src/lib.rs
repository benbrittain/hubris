#![no_std]

use derive_idol_err::IdolError;
use serde::{Deserialize, Serialize};
use userlib::*;
use zerocopy::{AsBytes, FromBytes};

pub type DnsQueryHandle = usize;

//const TIMER_INTERVAL: u64 = 1000;
//const AETHER_MASK: u32 = 1 << 0;
//const TIMER_MASK: u32 = 1 << 4;

impl Aether {
    pub fn resolve(&self, url: &str) -> Result<Ipv6Address, AetherError> {
        self.start_resolve_query(url.as_bytes())?;

        loop {
            // let deadline = sys_get_timer().now + TIMER_INTERVAL;
            // sys_set_timer(Some(deadline), TIMER_MASK);
            match self.resolve_query() {
                Ok(ip) => {
                    return Ok(ip);
                },
                Err(AetherError::QueueEmpty) => {
                    // Our incoming queue is empty. Wait for more packets.
                    //loop {
                        //sys_recv_closed(&mut [], 1, TaskId::KERNEL).unwrap();
                        //let msginfo = sys_recv_open(&mut [], AETHER_MASK);
                        //sys_log!("HELLO {}", msginfo.sender);
                        //if msginfo.sender == TaskId::KERNEL {
                        //}
                        //    // we got a timeout and we should resend the query
                        //    if msginfo.operation & 1 != TIMER_MASK {
                        //        self.start_resolve_query(url.as_bytes())?;
                        //    }
                        //    break;
                        //}
                    //}
                }
                Err(e) => return Err(e),
            }
        }
    }
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct UdpMetadata {
    pub addr: Ipv6Address,
    pub port: u16,
    pub payload_len: u32,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct TcpMetadata {
    pub addr: Ipv6Address,
    pub port: u16,
}

impl From<TcpMetadata> for smoltcp::wire::IpEndpoint {
    fn from(m: TcpMetadata) -> Self {
        Self {
            addr: m.addr.into(),
            port: m.port,
        }
    }
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
    /// No Packets to recieve. Will not wake task until there is a packet.
    QueueEmpty = 1,
    /// Can't transmit bytes because queue is full.
    QueueFull,
    /// No space in the transmit buffer.
    NoTransmitSlot,
    /// This socket is owned by a different task (check app.toml).
    WrongOwner,
    /// Attempted to make a TCP Socket action on a UDP Socket (or vice versa).
    WrongSocketType,
    /// The remote side of the TCP connection was closed.
    RemoteTcpClose,
    /// Failed to connect a TCP socket.
    TcpFailConnect,
    /// Error when attempting to send the packet.
    SendError,
    /// Aether only supports working with IPv6.
    NotIpv6,
    /// Dns resolution failed for some reason.
    DnsFailure,
    /// No DNS query has been requested to be resolved.
    NoPendingDnsQuery,
    /// We can only handle a single DNS query at a time.
    DnsQueryAlreadyInflight,
    /// Unknown Error from smoltcp.
    Unknown,
}

include!(concat!(env!("OUT_DIR"), "/client_stub.rs"));
include!(concat!(env!("OUT_DIR"), "/aether_config.rs"));
