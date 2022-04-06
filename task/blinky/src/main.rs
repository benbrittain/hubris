// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#![no_std]
#![no_main]

use drv_nrf52_gpio_api::*;
use drv_nrf52_uart_api::*;
use userlib::*;

task_slot!(GPIO, gpio);
task_slot!(UART, uart);

#[export_name = "main"]
fn main() -> ! {
    const TIMER_NOTIFICATION: u32 = 1;
    const INTERVAL: u64 = 500;

    let gpio = Gpio::from(GPIO.get_task_id());
    let uart = Uart::from(UART.get_task_id());

    sys_log!("Hello from blinky");


    let _ = gpio.configure(
        Port(1),
        Pin(10),
        Mode::Output,
        OutputType::PushPull,
        Pull::None,
    );
    let _ = gpio.configure(
        Port(1),
        Pin(15),
        Mode::Output,
        OutputType::PushPull,
        Pull::None,
    );
    let _ = gpio.toggle(Port(1), Pin(15));

    let mut msg = [0; 16];
    let mut dl = INTERVAL;

    sys_set_timer(Some(dl), TIMER_NOTIFICATION);
    loop {
        let msginfo = sys_recv_open(&mut msg, TIMER_NOTIFICATION);

        if msginfo.sender == TaskId::KERNEL {
            dl += INTERVAL;
            sys_set_timer(Some(dl), TIMER_NOTIFICATION);

            let text: &[u8] = b"123456789ABCDE\r\n";
            let resp = uart.write(text);
            sys_log!("resp: {:?}", resp);

            let text: &[u8] = b"alfalfa\r\n";
            let resp = uart.write(text);
            sys_log!("resp: {:?}", resp);
            //let resp = uart.write(text);
            //sys_log!("resp: {:?}", resp);

            loop {}
            //let mut in_buffer = [0; 20];
            //let resp= uart.read(&mut in_buffer);

            let _ = gpio.toggle(Port(1), Pin(10));
            let _ = gpio.toggle(Port(1), Pin(15));
        } else {
            sys_panic(b"nothing besides the kernel should be talking to this!");
        }
    }
}
