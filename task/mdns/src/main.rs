//! A Mdns resolver
//!
//! https://www.rfc-editor.org/rfc/rfc6762

#![no_std]
#![no_main]

use userlib::*;

mod server;
mod dispatch;

task_slot!(AETHER, aether);

use task_aether_api::*;

#[export_name = "main"]
fn main() -> ! {
    let aether = Aether::from(AETHER.get_task_id());

    let mut server = server::MdnsServer::default();
    let mut msgbuf = [0u8; server::INCOMING_SIZE];

    const SOCKET: SocketName = SocketName::mdns;

    loop {
        let mut rx_data_buf = [0u8; 64];
        match aether.recv_udp_packet(SOCKET, &mut rx_data_buf) {
            Ok(metadata) => {
                let msg = dnsparse::Message::parse(
                    &mut rx_data_buf[..metadata.payload_len as usize],
                );
                sys_log!("{:?}", msg);
            }
            // There is not a packet waiting, so let's do some.
            // idl handling.
            Err(AetherError::QueueEmpty) => {
                // Our incoming queue is empty. Wait for more packets.
                //sys_recv_closed(&mut [], 1, TaskId::KERNEL).unwrap();
            }
            _ => panic!(),
        }
        dispatch::dispatch(&mut rx_data_buf, &mut server);
    }
}
