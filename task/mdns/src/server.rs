use core::fmt::Display;

use task_mdns_api::*;

use crate::server::idl::InOrderMdnsImpl;
use dnsparse::*;
use heapless::FnvIndexMap;
pub use idl::INCOMING_SIZE;
use idol_runtime::{Server, ServerOp};
use task_aether_api::UdpMetadata;
use userlib::*;

pub struct MdnsServer<'a> {
    hostname: &'a str,
    cache: FnvIndexMap<HostName, Ipv6Address, 4>,
}

impl<'a> MdnsServer<'a> {
    pub fn new(hostname: &'a str) -> MdnsServer<'a> {
        // TODO remove hardcode ip
        let mut m = MdnsServer {
            hostname,
            cache: FnvIndexMap::new(),
        };
        m.cache.insert(
            "portal.local".into(),
            [0xfd, 0x00, 0x1e, 0xaf, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x1]
                .into(),
        );
        m
    }
}

//fn respond_us() -> &Message {
//
//
//}

impl MdnsServer<'_> {
    pub fn process_query(
        &mut self,
        msg: Message<'_>,
        out_buf: &mut [u8],
        metadata: UdpMetadata,
    ) -> Option<usize> {
        for question in msg.questions() {
            match question.kind() {
                QueryKind::AAAA => {
                    sys_log!("> AAAA {}", question.name());
                    let hostname = HostName::from_buf(|buf| {
                        question.name().read_to_buf(buf)
                    });

                    let header = HeaderBuilder::default()
                        .id(msg.header().id())
                        .kind(HeaderKind::Response)
                        .response_code(ResponseCode::NoError)
                        .build();

                    //let answer = Answer::builder()
                    //    .class(QueryClass::In);


                    // respond to the dns request we got
                    let mut builder = Message::builder(out_buf);
                    let msg = builder.header(header).build();
                    //msg.add_answer(answer);

                    return Some(msg.len());
//                    sys_log!("send: {:?}", msg);

                    // sys_log!("HOSTNAME: {:?}", hostname);
                    // self.cache.insert(hostname, metadata.addr);
                }
                _ => sys_log!("> UNHANDLED {:?}", question),
            }
        }
        None
    }

    //if hostname == *self.hostname {
    //    return Some(respond_us());
    //}

    pub fn process_msg(
        &mut self,
        r: Message<'_>,
        out_buf: &mut [u8],
        metadata: UdpMetadata,
    ) -> Option<usize> {
        sys_log!("processing mdns message");
        match r.header().opcode() {
            OpCode::Query => {
                self.process_query(r, out_buf, metadata)
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

impl idl::InOrderMdnsImpl for MdnsServer<'_> {
    fn resolve(
        &mut self,
        msg: &userlib::RecvMessage,
        hostname: HostName,
    ) -> Result<Ipv6Address, idol_runtime::RequestError<MdnsError>> {
        if let Some(entry) = self.cache.get(&hostname) {
            return Ok(entry.clone());
        }
        Err(MdnsError::HostNotFound.into())
    }
}

pub(crate) mod idl {
    use task_mdns_api::*;
    include!(concat!(env!("OUT_DIR"), "/server_stub.rs"));
}
