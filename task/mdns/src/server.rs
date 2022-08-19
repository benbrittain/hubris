use task_mdns_api::*;

use crate::server::idl::InOrderMdnsImpl;
pub use idl::INCOMING_SIZE;
use idol_runtime::{Server, ServerOp};
use userlib::*;

#[derive(Default)]
pub struct MdnsServer {}

impl MdnsServer {}

impl idl::InOrderMdnsImpl for MdnsServer {
    fn resolve(
        &mut self,
        msg: &userlib::RecvMessage,
        socket: HostName,
    ) -> Result<Ipv6Address, idol_runtime::RequestError<MdnsError>> {
        Ok([0; 16].into())
    }
}

pub(crate) mod idl {
    use task_mdns_api::*;
    include!(concat!(env!("OUT_DIR"), "/server_stub.rs"));
}
