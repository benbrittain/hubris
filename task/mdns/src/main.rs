//! A Mdns resolver
//!
//! https://www.rfc-editor.org/rfc/rfc6762

#![no_std]
#![no_main]

use userlib::*;

mod dispatch;
mod server;

task_slot!(AETHER, aether);

use task_aether_api::*;

#[export_name = "main"]
fn main() -> ! {
    let aether = Aether::from(AETHER.get_task_id());

    let mut server = server::MdnsServer::new(aether, "aether.local");
    let mut msgbuf = [0u8; server::INCOMING_SIZE];

    const SOCKET: SocketName = SocketName::mdns;

    loop {
        let mut rx_data_buf = [0u8; 64];
        match aether.recv_udp_packet(SOCKET, &mut rx_data_buf) {
            Ok(metadata) => {
                if let Ok(msg) = dnsparse::Message::parse(
                    &mut rx_data_buf[..metadata.payload_len as usize],
                ) {
                    server.process_msg(msg, metadata);
                }
            }
            Err(AetherError::QueueEmpty) => {
                // This is where we usually close_recv,
                // but we need to dispatch for server handling
                // so that function is where we yield
            }
            _ => panic!(),
        }
        dispatch::dispatch(&mut rx_data_buf, &mut server);
    }
}
