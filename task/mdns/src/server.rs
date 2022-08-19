use task_mdns_api::*;

pub use idl::INCOMING_SIZE;

#[derive(Default)]
pub struct MdnsServer {}

impl idl::InOrderMdnsImpl for MdnsServer {
    fn resolve(
        &mut self,
        msg: &userlib::RecvMessage,
        socket: HostName,
    ) -> Result<Ipv6Address, idol_runtime::RequestError<MdnsError>> {
        todo!()
    }
}

mod idl {
    use task_mdns_api::*;
    include!(concat!(env!("OUT_DIR"), "/server_stub.rs"));
}
