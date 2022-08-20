use task_mdns_api::*;

use heapless::FnvIndexMap;
use crate::server::idl::InOrderMdnsImpl;
use dnsparse::*;
pub use idl::INCOMING_SIZE;
use idol_runtime::{Server, ServerOp};
use userlib::*;
use task_aether_api::UdpMetadata;

pub struct MdnsServer {
//    hostname:
    cache: FnvIndexMap::<HostName, Ipv6Address, 4>,
}

impl MdnsServer {
    pub fn new() -> Self {
        // TODO remove hardcode ip
        let mut m = MdnsServer { cache: FnvIndexMap::new() };
        m.cache.insert(
            "portal.local".into(),
            [0xfd, 0x00, 0x1e, 0xaf, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x1]
                .into(),
        );
        m
    }

}

impl MdnsServer {
    pub fn process_msg(&mut self, r: Message<'_>, metadata: UdpMetadata) {
        sys_log!("processing mdns message");
        match r.header().opcode() {
            OpCode::Query => {
                for question in r.questions() {
                    match question.kind() {
                        QueryKind::AAAA => {
                            sys_log!("> AAAA {}", question.name());
                            self.cache.insert(3, metadata.addr);

                        }
                        _=> sys_log!("> UNHANDLED {:?}", question),
                    }

                }
            }
            o => panic!("Don't know how to handle this opcode: {:?}", o),
            //OpCode::Status => {}
            //OpCode::Notify => {}
            //OpCode::InverseQuery => {}
            //OpCode::Update => {}
            //OpCode::Reserved(_) => {}
        }
    }
}

impl idl::InOrderMdnsImpl for MdnsServer {
    fn resolve(
        &mut self,
        msg: &userlib::RecvMessage,
        hostname: HostName,
    ) -> Result<Ipv6Address, idol_runtime::RequestError<MdnsError>> {
        if let Some(entry) = self.cache.get(&hostname){
            return Ok(entry.clone());
        }
        Err(MdnsError::HostNotFound.into())
    }
}

pub(crate) mod idl {
    use task_mdns_api::*;
    include!(concat!(env!("OUT_DIR"), "/server_stub.rs"));
}
