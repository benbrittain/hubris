// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#![no_std]
#![no_main]

use minimq::{self, Minimq, QoS, Retain};
use task_aether_api::*;
use userlib::*;

mod tcp_interop;
mod clock_interop;

use tcp_interop::NetworkLayer;
use clock_interop::ClockLayer;

task_slot!(AETHER, aether);

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
            match topic {
                "topic" => {
                    let string = match core::str::from_utf8(message) {
                        Ok(v) => v,
                        Err(e) => panic!("Invalid UTF-8 sequence: {}", e),
                    };
                    sys_log!("mqtt> '{}': '{}'", topic, string);
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
