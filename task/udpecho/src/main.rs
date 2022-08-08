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
    let aether = AETHER.get_task_id();
    let aether = Aether::from(aether);

    const SOCKET: SocketName = SocketName::echo;

    aether.tcp_listen(SOCKET, 7);
    loop {
        // Tiiiiiny payload buffer
        let mut rx_data_buf = [0u8; 1280];
        sys_log!("========================= RECV =========================");
        match aether.recv_tcp_data(SOCKET, &mut rx_data_buf) {
            Ok(payload_len) => {
                //// A packet! We want to turn it right around. Deserialize the
                //// packet header; unwrap because we trust the server.
                //UDP_ECHO_COUNT
                //    .fetch_add(1, core::sync::atomic::Ordering::Relaxed);

                let s = match core::str::from_utf8(
                    &rx_data_buf[..payload_len as usize],
                ) {
                    Ok(v) => v,
                    Err(e) => panic!("Invalid UTF-8 sequence: {}", e),
                };

                sys_log!("data: {}", s);

                // Now we know how many bytes to return.
                let tx_bytes = &rx_data_buf[..payload_len as usize];

                aether.send_tcp_data(SOCKET, tx_bytes).unwrap();
            }
            Err(AetherError::RemoteTcpClose) => {
                sys_log!("Connection is Closed!");
                aether.close_tcp(SOCKET).unwrap();
                aether.tcp_listen(SOCKET, 7).unwrap();
                sys_recv_closed(&mut [], 1, TaskId::KERNEL).unwrap();
            }
            Err(AetherError::QueueEmpty) => {
                // Our incoming queue is empty. Wait for more packets.
                sys_recv_closed(&mut [], 1, TaskId::KERNEL).unwrap();
            }
            _ => panic!(),
        }

        // Try again.
    }
}

static UDP_ECHO_COUNT: core::sync::atomic::AtomicU32 =
    core::sync::atomic::AtomicU32::new(0);
