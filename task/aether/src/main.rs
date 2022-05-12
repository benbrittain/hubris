//! A 6LowPAN implementation over 802.15.4

#![no_std]
#![no_main]

use userlib::sys_log;

use smoltcp::{
    iface::{
        FragmentsCache, InterfaceBuilder, Neighbor, NeighborCache,
        PacketAssembler,
    },
    phy::Medium,
    socket::{UdpPacketMetadata, UdpSocket, UdpSocketBuffer},
    storage::RingBuffer,
    time::Instant,
    wire::{Ieee802154Pan, IpAddress, IpCidr, SixlowpanFragKey},
};

mod server;

static mut UDP_BUFFER_TX: [UdpPacketMetadata; 8] =
    [UdpPacketMetadata::EMPTY; 8];
static mut UDP_BUFFER_TX2: [u8; 64] = [0; 64];
static mut UDP_BUFFER_RX: [UdpPacketMetadata; 8] =
    [UdpPacketMetadata::EMPTY; 8];
static mut UDP_BUFFER_RX2: [u8; 128] = [0; 128];

/// 802.15.4 PAN ID
const PAN_ID: u16 = 0x1eaf; // leaf

/// Number of entries to maintain in our neighbor cache (ARP/NDP).
const NEIGHBORS: usize = 4;

/// Number of sockets to support
/// TODO if I want to increase this, I need to manage the udp_handle properly
/// instead of just using static muts for udp buffers
const SOCKET_COUNT: usize = 1;

/// Notification mask for our IRQ; must match configuration in app.toml.
const RADIO_IRQ: u32 = 1;

// hacky way to get logs out of smoltcp
static SYS_LOGGER: SysLogger = SysLogger;
struct SysLogger;
impl log::Log for SysLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        sys_log!("{} - {}", record.level(), record.args());
    }
    fn flush(&self) {}
}

#[export_name = "main"]
fn main() -> ! {
    // Setup a logger shim so we can see the output of smoltcp.
    log::set_logger(&SYS_LOGGER).unwrap();
    log::set_max_level(log::LevelFilter::Trace);

    // Start up the radio.
    let mut radio = nrf52_radio::Radio::new();
    radio.initialize();

    // Derive an IP address for our WPAN using IEEE UEI-64.
    let ipv6_addr = IpAddress::Ipv6(radio.get_ieee_uei_64().into());
    let mut ip_addrs = [IpCidr::new(ipv6_addr, 64)];

    // TODO CHECK WHAT THIS IS BEN
    let ieee802154_addr = smoltcp::wire::Ieee802154Address::Extended([
        0x1a, 0x0b, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42,
    ]);

    let mut neighbor_cache_storage: [Option<(IpAddress, Neighbor)>; NEIGHBORS] =
        [None; NEIGHBORS];
    let neighbor_cache = NeighborCache::new(&mut neighbor_cache_storage[..]);

    let mut packet_buf = [0u8, 127];
    let mut packet_assembler_cache =
        [PacketAssembler::<'_>::new(&mut packet_buf[..])];
    let mut packet_index_cache: [Option<(SixlowpanFragKey, usize)>; 1] = [None];
    let fragments_cache = FragmentsCache::new(
        &mut packet_assembler_cache[..],
        &mut packet_index_cache[..],
    );

    let mut cache_buf = [0u8, 127];
    let mut out_fragments_cache = [(0usize, (&mut cache_buf[..]).into())];

    let mut sockets: [_; generated::SOCKET_COUNT] = Default::default();
    let mut iface = InterfaceBuilder::new(radio, &mut sockets[..])
        .pan_id(generated::PAN_ID)
        .hardware_addr(ieee802154_addr.into())
        .neighbor_cache(neighbor_cache)
        .sixlowpan_fragments_cache(fragments_cache)
        .out_fragments_cache(RingBuffer::new(&mut out_fragments_cache[..]))
        .ip_addrs(&mut ip_addrs[..])
        .finalize();

    let sockets = generated::construct_sockets();
    let mut socket_handles = [None; generated::SOCKET_COUNT];
    for (socket, h) in sockets.0.into_iter().zip(&mut socket_handles) {
        *h = Some(iface.add_socket(socket));
    }
    let socket_handles = socket_handles.map(|h| h.unwrap());

    // Bind sockets to their ports.
    for (&h, &port) in socket_handles.iter().zip(&generated::SOCKET_PORTS) {
        iface
            .get_socket::<UdpSocket>(h)
            .bind((ipv6_addr, port))
            .map_err(|_| ())
            .unwrap();
    }

    sys_log!("Starting the Aether server...");

    userlib::sys_irq_control(RADIO_IRQ, true);
    let mut server = server::AetherServer::new(socket_handles, iface);

    loop {
        let poll_result = server
            .interface_mut()
            .poll(Instant::from_millis(userlib::sys_get_timer().now as i64));
        let activity = poll_result.unwrap_or(true);

        if activity {
            // TODO wake up any clients that have a closed recv on this server
            for i in 0..crate::SOCKET_COUNT {
                // TODO check if there is a packet on the port
                if server.get_socket_mut(i).is_ok() {
                    let (task_id, notification) = generated::SOCKET_OWNERS[i];
                    let task_id = userlib::sys_refresh_task_id(task_id);
                    userlib::sys_post(task_id, notification);
                }
            }
        } else {
            let mut msgbuf = [0u8; server::INCOMING_SIZE];
            idol_runtime::dispatch_n(&mut msgbuf, &mut server);
        }
    }
}

mod generated {
    include!(concat!(env!("OUT_DIR"), "/aether_config.rs"));
}
