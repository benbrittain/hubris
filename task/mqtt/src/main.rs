// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#![no_std]
#![no_main]

use minimq::{self, Minimq, QoS, Retain};
use task_aether_api::*;
use userlib::*;

task_slot!(AETHER, aether);

struct NetworkLayer {
    aether: Aether,
    socket: SocketName,
}

use minimq::embedded_nal::nb::Error as MqError;
use minimq::embedded_nal::TcpClientStack;
use minimq::embedded_nal::{IpAddr, SocketAddr};

fn socket_addr_to_metadata(
    addr: SocketAddr,
) -> Result<TcpMetadata, AetherError> {
    let (ip, port) = match addr {
        SocketAddr::V6(addrv6) => {
            (Ipv6Address(addrv6.ip().octets()), addrv6.port())
        }
        _ => return Err(AetherError::Unknown),
    };
    Ok(TcpMetadata { addr: ip, port })
}

impl TcpClientStack for NetworkLayer {
    type TcpSocket = SocketName;
    type Error = AetherError;

    fn socket(
        &mut self,
    ) -> Result<
        <Self as TcpClientStack>::TcpSocket,
        <Self as TcpClientStack>::Error,
    > {
        sys_log!("minimq: socket");
        Ok(self.socket)
    }
    fn connect(
        &mut self,
        socket: &mut <Self as TcpClientStack>::TcpSocket,
        addr: SocketAddr,
    ) -> Result<(), MqError<<Self as TcpClientStack>::Error>> {
        // TODO impl from for addr
        sys_log!("minimq: connect");
        self.aether
            .tcp_connect(self.socket, socket_addr_to_metadata(addr)?)
            .map_err(|e| MqError::Other(e))
    }
    fn is_connected(
        &mut self,
        socket: &<Self as TcpClientStack>::TcpSocket,
    ) -> Result<bool, <Self as TcpClientStack>::Error> {
        userlib::hl::sleep_for(100);
        let active = self.aether.is_tcp_active(*socket);
        sys_log!("minimq: is_connected {:?}", active);
        active
    }

    fn send(
        &mut self,
        socket: &mut <Self as TcpClientStack>::TcpSocket,
        bytes: &[u8],
    ) -> Result<usize, MqError<<Self as TcpClientStack>::Error>> {
        sys_log!("trying to send: {:x?}", bytes);
        let r = self.aether.send_tcp_data(self.socket, bytes);
        sys_log!("send {:?}", r);
        r.map_err(|e| MqError::Other(e)).map(|e| e as usize)
    }
    fn receive(
        &mut self,
        socket: &mut <Self as TcpClientStack>::TcpSocket,
        bytes: &mut [u8],
    ) -> Result<usize, MqError<<Self as TcpClientStack>::Error>> {
        loop {
            match self.aether.recv_tcp_data(self.socket, bytes) {
                Ok(len) => {
                    return Ok(len as usize);
                }
                Err(AetherError::QueueEmpty) => {
                    // Our incoming queue is empty. Wait for more packets.
                    sys_recv_closed(&mut [], 1, TaskId::KERNEL).unwrap();
                }
                e => {
                    sys_log!("Unknown recv: {:?}", e);
                    break;
                }
            }
        }
        Err(MqError::Other(AetherError::Unknown))
    }

    fn close(
        &mut self,
        socket: <Self as TcpClientStack>::TcpSocket,
    ) -> Result<(), <Self as TcpClientStack>::Error> {
        sys_log!("minimq: close");
        todo!()
    }
}

struct ClockLayer {}

use minimq::embedded_time::fraction::Fraction;
impl minimq::embedded_time::Clock for ClockLayer {
    type T = u32;

    const SCALING_FACTOR: Fraction = Fraction::new(1, 1000);

    fn try_now(
        &self,
    ) -> Result<
        minimq::embedded_time::Instant<Self>,
        minimq::embedded_time::clock::Error,
    > {
        Ok(minimq::embedded_time::Instant::<ClockLayer>::new(0))
    }
}

#[export_name = "main"]
fn main() -> ! {
    let aether = Aether::from(AETHER.get_task_id());
    let mut mqtt: Minimq<_, _, 256, 16> = Minimq::new(
        "fd00:1eaf::1".parse().unwrap(),
        "mqtt-aether",
        NetworkLayer {
            aether,
            socket: SocketName::mqtt,
        },
        ClockLayer {},
    )
    .unwrap();

    let mut subscribed = false;

    loop {
        if mqtt.client.is_connected() && !subscribed {
            mqtt.client.subscribe("topic", &[]);
            subscribed = true;
        }

        mqtt.poll(|client, topic, message, properties| {
            sys_log!("here 4");
            match topic {
                "topic" => {
                    sys_log!("{:?}", message);
                    client
                        .publish(
                            "echo",
                            message,
                            QoS::AtMostOnce,
                            Retain::NotRetained,
                            &[],
                        )
                        .unwrap();
                }
                topic => sys_log!("Unknown topic: {}", topic),
            };
        })
        .unwrap();
    }
}
