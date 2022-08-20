#![no_std]

use derive_idol_err::IdolError;
use serde::{Deserialize, Serialize};
use userlib::*;
use zerocopy::{AsBytes, FromBytes};
pub use task_aether_api::Ipv6Address;

#[derive(Copy, Clone, Debug, PartialEq, FromPrimitive, IdolError)]
#[repr(u32)]
pub enum MdnsError {
    /// Failed to resolve host name.
    HostNotFound = 1,
}

/// NOTE this should be 255, but derives aren't automatically
/// done for that, so punt till a problem.
pub const MAX_HOSTNAME_LEN: usize = 16;

#[derive(Hash, Eq, Copy, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[repr(C)]
pub struct HostName([u8; MAX_HOSTNAME_LEN]);

impl From<&str> for HostName {
    fn from(hostname: &str) -> Self {
        let len = if hostname.len() > MAX_HOSTNAME_LEN {
            panic!("FINISH THIS BEN, WE CAN GO UP TO 255");
            MAX_HOSTNAME_LEN
        } else {
            hostname.len()
        };

        let mut out_hostname = HostName::default();
        out_hostname.0[..len].copy_from_slice(&hostname.as_bytes()[..len]);
        out_hostname
    }
}

include!(concat!(env!("OUT_DIR"), "/client_stub.rs"));
