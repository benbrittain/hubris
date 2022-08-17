//! A 6LowPAN implementation over 802.15.4

#![no_std]
#![no_main]

use drv_rng_api;
use smoltcp::{
    iface::{
        FragmentsCache, InterfaceBuilder, Neighbor, NeighborCache,
        PacketAssembler, SocketSet,
    },
    phy::Medium,
    socket::{tcp, udp},
    storage::RingBuffer,
    time::Instant,
    wire::{
        Ieee802154Address, Ieee802154Pan, IpAddress, IpCidr, SixlowpanFragKey,
    },
};
use userlib::sys_log;

// hacky way to get logs out of smoltcp
#[cfg(feature = "log-smoltcp")]
static SYS_LOGGER: log_smoltcp::SysLogger = log_smoltcp::SysLogger;

userlib::task_slot!(RNG, rng_driver);

#[cfg(feature = "log-smoltcp")]
mod log_smoltcp;
mod server;

/// IEEE 802.15.4 link MTU
const IEEE802_LINK_MTU: usize = 1280;

/// Number of entries to maintain in our neighbor cache (ARP/NDP).
const NEIGHBORS: usize = 4;

/// Notification mask for our IRQ; must match configuration in app.toml.
const RADIO_IRQ: u32 = 1;

const MAX_FRAGMENTS: usize = 2;

static mut PACKET_FRAGMENTS: [[u8; IEEE802_LINK_MTU]; MAX_FRAGMENTS] =
    [[0; IEEE802_LINK_MTU]; MAX_FRAGMENTS];

#[export_name = "main"]
fn main() -> ! {
    let rng = drv_rng_api::Rng::from(RNG.get_task_id());
    // Setup a logger shim so we can see the output of smoltcp.
    #[cfg(feature = "log-smoltcp")]
    log::set_logger(&SYS_LOGGER).unwrap();
    #[cfg(feature = "log-smoltcp")]
    log::set_max_level(log::LevelFilter::Trace);

    // Start up the radio.
    let mut radio = nrf52_radio::Radio::new();
    radio.initialize();

    // Derive an IP address for our WPAN using IEEE UEI-64.
    let ieee802154_addr: Ieee802154Address = radio.get_addr().into();
    let ieee802154_addr_short: Ieee802154Address =
        smoltcp::wire::Ieee802154Address::Short([0x00, 0x08]);

    // TODO We should set a link local address when we have SLAAC/NDISC working.
    let link_local_ipv6_addr =
        IpAddress::Ipv6(ieee802154_addr.as_link_local_address().unwrap());

    let mut site_local_ip_bytes = [0; 16];
    // big endian so the ip addr looks pretty and like the pan_id
    let pan_id_bytes = generated::PAN_ID.0.to_be_bytes();
    site_local_ip_bytes[..8].copy_from_slice(&[
        0xfd,
        0x00,
        pan_id_bytes[0],
        pan_id_bytes[1],
        0x00,
        0x00,
        0x00,
        0x00,
    ]);
    site_local_ip_bytes[8..].copy_from_slice(&radio.get_addr().0);
    let site_local_ipv6_addr = IpAddress::Ipv6(
        smoltcp::wire::Ipv6Address::from_bytes(&site_local_ip_bytes),
    );
    let mut ip_addrs = [
        IpCidr::new(site_local_ipv6_addr, 64),
//        IpCidr::new(link_local_ipv6_addr, 64),
    ];
    for addr in ip_addrs {
        sys_log!("IP addr: {}", addr);
    }

    let mut neighbor_cache_storage: [Option<(IpAddress, Neighbor)>; NEIGHBORS] =
        [None; NEIGHBORS];
    let neighbor_cache = NeighborCache::new(&mut neighbor_cache_storage[..]);

    let mut packet_assembler_cache = unsafe {
        [
            PacketAssembler::<'_>::new(&mut PACKET_FRAGMENTS[0][..]),
            PacketAssembler::<'_>::new(&mut PACKET_FRAGMENTS[1][..]),
        ]
    };
    let mut packet_index_cache: [Option<(SixlowpanFragKey, usize)>;
        MAX_FRAGMENTS] = [None; MAX_FRAGMENTS];
    let fragments_cache = FragmentsCache::new(
        &mut packet_assembler_cache[..],
        &mut packet_index_cache[..],
    );

    let mut out_packet_buffer = [0u8; IEEE802_LINK_MTU];

    let mut builder = InterfaceBuilder::new()
        .ip_addrs(&mut ip_addrs[..])
        .pan_id(generated::PAN_ID);
    builder = builder
        .hardware_addr(ieee802154_addr.into())
        .neighbor_cache(neighbor_cache)
        .sixlowpan_fragments_cache_timeout(smoltcp::time::Duration::from_secs(
            2,
        ))
        .sixlowpan_fragments_cache(fragments_cache)
        .sixlowpan_out_packet_cache(&mut out_packet_buffer[..]);
    let mut iface = builder.finalize(&mut radio);

    let mut socket_storage = [Default::default(); generated::SOCKET_COUNT];
    let mut socket_set = SocketSet::new(&mut socket_storage[..]);

    let sockets = generated::construct_sockets();
    let mut socket_handles = [None; generated::SOCKET_COUNT];
    for (socket, h) in sockets.udp.into_iter().zip(&mut socket_handles) {
        *h = Some(server::SocketHandleType::Udp(socket_set.add(socket)));
    }
    for (socket, h) in sockets.tcp.into_iter().zip(&mut socket_handles) {
        *h = Some(server::SocketHandleType::Tcp(socket_set.add(socket)));
    }
    let socket_handles = socket_handles.map(|h| h.unwrap());

    // Bind sockets to their ports.
    for (&handle, &port) in
        socket_handles.iter().zip(&generated::UDP_SOCKET_PORTS)
    {
        match handle {
            server::SocketHandleType::Udp(handle) => {
                let udp_socket = socket_set.get_mut::<udp::Socket>(handle);
                udp_socket.bind((site_local_ipv6_addr, port)).unwrap();
            }
            server::SocketHandleType::Tcp(handle) => {}
        }
    }

    userlib::sys_irq_control(RADIO_IRQ, true);
    let mut server = server::AetherServer::new(
        socket_handles,
        socket_set,
        iface,
        radio,
        rng,
    );

    loop {
        let poll_result = server
            .poll(Instant::from_millis(userlib::sys_get_timer().now as i64));
        let activity = poll_result.unwrap_or(true);

        if activity {
            for i in 0..generated::SOCKET_COUNT {
                // TODO check if there is a packet on the port
                //if server.get_socket_mut(i).is_ok() {
                let (task_id, notification) = generated::SOCKET_OWNERS[i];
                let task_id = userlib::sys_refresh_task_id(task_id);
                userlib::sys_post(task_id, notification);
                //}
            }
            // TODO poll at timing perhaps?
        } else {
            let mut msgbuf = [0u8; server::INCOMING_SIZE];
            idol_runtime::dispatch_n(&mut msgbuf, &mut server);
        }
    }
}

mod generated {
    include!(concat!(env!("OUT_DIR"), "/aether_config.rs"));
}
