// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#![no_std]
#![no_main]

use task_aether_api::*;
use userlib::*;

task_slot!(AETHER, aether);

#[export_name = "main"]
fn main() -> ! {
    let net = Aether::from(AETHER.get_task_id());

    sys_log!("starting aetherping");
    let tx = [0xBB, 0xBB, 0xBB];

    //let meta = UdpMetadata {
    //    // IPv6 multicast address for "all routers"
    //    addr: Address::Ipv6(Ipv6Address([
    //        0xff, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2,
    //    ])),
    //    port: 8,
    //    payload_len: tx.len() as u32,
    //};

    let ip = net.get_addr();
    sys_log!("generated an ip: {:?}", ip);

    let mut rx_data_buf = [0u8; 128];
    loop {

        sys_log!("RECV");
        match net.recv_packet(SocketName::ping, &mut rx_data_buf) {
            Ok(meta) => {
                let addr: smoltcp::wire::IpAddress = meta.addr.into();
                sys_log!("packet from: {}", addr);
                //sys_log!("{:?}", &rx_data_buf[..meta.payload_len as usize]);

                sys_log!("RESP");
                net.send_packet(SocketName::ping, meta, &tx[..]);
                hl::sleep_for(2000);
            }
            Err(AetherError::QueueEmpty) => {
                // Our incoming queue is empty. Wait for more packets.
                sys_recv_closed(&mut [], 5, TaskId::KERNEL).unwrap();
            }
            _ => panic!("oh no!"),
        }
        //hl::sleep_for(2000);
    }
}
