// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A SLIP Interface to the Aether

#![no_std]
#![no_main]

use drv_nrf52_uart_api::Uart;
use serial_line_ip::Decoder;
use smoltcp::wire::Ipv6Repr;
use task_aether_api::*;
use userlib::*;

task_slot!(UART, uart);
task_slot!(AETHER, aether);

const SOCKET: SocketName = SocketName::slip;

const AETHER_NOTIFICATION: u32 = 1;
const UART_POLL_FREQ_NOTIFICATION: u32 = 2;
const UART_POLL_FREQ: u64 = 400;

#[export_name = "main"]
fn main() -> ! {
    let uart = Uart::from(UART.get_task_id());
    let aether = Aether::from(AETHER.get_task_id());

    sys_log!("SLIP into the Aether");

    let mut output = [0; 2048];
    let mut buffer = [0; 2048];

    let mut rx_data_buf = [0u8; 256];

    let mut deadline = UART_POLL_FREQ;
    sys_set_timer(Some(deadline), UART_POLL_FREQ_NOTIFICATION);

    loop {
        // wake up if we get new packets or it's time to poll the uart service.
        let msginfo = sys_recv_closed(
            &mut [],
            AETHER_NOTIFICATION | UART_POLL_FREQ_NOTIFICATION,
            TaskId::KERNEL,
        )
        .unwrap();
        assert!(msginfo.sender == TaskId::KERNEL);

        if msginfo.operation & UART_POLL_FREQ_NOTIFICATION != 0 {
            deadline += UART_POLL_FREQ;
            sys_set_timer(Some(deadline), UART_POLL_FREQ_NOTIFICATION);

            // Read data off the uart.
            let uart_amount_read = uart.read(0, &mut buffer).unwrap();
            if uart_amount_read > 0 {
                let mut slip = Decoder::new();
                if let Ok((bytes_processed, output_slice, is_end_of_packet)) =
                    slip.decode(&buffer[..uart_amount_read], &mut output)
                {
                    if is_end_of_packet {
                        match smoltcp::wire::Ipv6Packet::new_checked(
                            output_slice,
                        ) {
                            Ok(packet) => {
                                let repr = Ipv6Repr::parse(&packet).unwrap();
                                let mut dest_addr = repr.dst_addr;

                                let udp_packet =
                                    smoltcp::wire::UdpPacket::new_checked(
                                        packet.payload(),
                                    )
                                    .unwrap();

                                let tx_bytes = udp_packet.payload();

                                let meta = UdpMetadata {
                                    addr: Ipv6Address(dest_addr.0),
                                    port: udp_packet.dst_port(),
                                    payload_len: udp_packet.payload().len()
                                        as u32,
                                };
                                aether.send_packet(SOCKET, meta, &tx_bytes);
                            }
                            Err(e) => sys_log!("err: {:?}", e),
                        }
                    }
                }
            }
        }

        loop {
            // Read data off the network
            match aether.recv_packet(SOCKET, &mut rx_data_buf) {
                Ok(meta) => {
                    sys_log!("SLIP - Recv Packet");
                    // Send data back up the uart
                    uart.write(&rx_data_buf[..meta.payload_len as usize]);
                }
                Err(AetherError::QueueEmpty) => {
                    sys_log!("SLIP - No packets, stalling.");
                    break;
                }
                _ => panic!("Unhandled error on recv packet"),
            }
        }
    }
}
