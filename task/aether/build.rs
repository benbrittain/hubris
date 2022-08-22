// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// almost directly copied from the build_net stuff, maybe integrate back
// in at some point

use proc_macro2::TokenStream;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    idol::server::build_server_support(
        "../../idl/aether.idol",
        "server_stub.rs",
        idol::server::ServerStyle::InOrder,
    )?;

    let aether_config = load_aether_config()?;
    generate_aether_config(&aether_config)?;

    Ok(())
}

#[derive(Deserialize)]
pub struct SocketConfig {
    pub kind: String,
    pub owner: TaskNote,
    pub port: Option<u16>,
    pub tx: BufSize,
    pub rx: BufSize,
}

#[derive(Deserialize)]
pub struct TaskNote {
    pub name: String,
    pub notification: u32,
}

#[derive(Deserialize)]
pub struct BufSize {
    pub packets: Option<usize>,
    pub bytes: usize,
}

#[derive(Deserialize)]
pub struct GlobalConfig {
    pub aether: AetherConfig,
}

pub type SocketMap = BTreeMap<String, SocketConfig>;

#[derive(Deserialize)]
pub struct AetherConfig {
    pub pan_id: u16,
    /// Sockets known to the system, indexed by name.
    pub sockets: Option<SocketMap>,
}

pub fn load_aether_config() -> Result<AetherConfig, Box<dyn std::error::Error>>
{
    Ok(build_util::config::<GlobalConfig>()?.aether)
}

pub fn generate_socket_enum(
    config: &AetherConfig,
    mut out: impl std::io::Write,
) -> Result<(), std::io::Error> {
    if let Some(sockets) = &config.sockets {
        writeln!(out, "#[allow(non_camel_case_types)]")?;
        writeln!(out, "#[repr(u8)]")?;
        writeln!(
        out,
        "#[derive(Copy, Clone, Debug, Eq, PartialEq, userlib::FromPrimitive)]"
    )?;
        writeln!(out, "#[derive(serde::Serialize, serde::Deserialize)]")?;
        writeln!(out, "pub enum SocketName {{")?;
        for (i, name) in sockets.keys().enumerate() {
            writeln!(out, "    {} = {},", name, i)?;
        }
        writeln!(out, "}}")?;
    }
    Ok(())
}

fn generate_aether_config(
    config: &AetherConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = std::env::var("OUT_DIR")?;
    let dest_path = std::path::Path::new(&out_dir).join("aether_config.rs");

    let mut out = std::fs::File::create(&dest_path)?;

    let socket_count = if let Some(sockets) = &config.sockets {
        sockets.len()
    } else {
        0
    };
    writeln!(
        out,
        "{}",
        quote::quote! {
            #[allow(unused_imports)]
            use core::sync::atomic::{AtomicBool, Ordering};
            #[allow(unused_imports)]
            use smoltcp::socket::{tcp, udp};

            /// Number of sockets (excluding DNS)
            pub const SOCKET_COUNT: usize = #socket_count;
        }
    )?;

    if let Some(sockets) = &config.sockets {
        for (name, socket) in sockets {
            writeln!(out, "{}", generate_socket_state(name, socket)?)?;
        }
    }

    let pan_id = config.pan_id;
    writeln!(
        out,
        "{}",
        quote::quote! {
            #[allow(unused_imports)]
            use smoltcp::wire::Ieee802154Pan;
            pub const PAN_ID: Ieee802154Pan = Ieee802154Pan(#pan_id);
        }
    )?;

    if let Some(sockets) = &config.sockets {
        writeln!(out, "{}", generate_state_struct(&sockets)?)?;
        writeln!(out, "{}", generate_constructor(&sockets)?)?;
        writeln!(out, "{}", generate_port_table(&sockets)?)?;
    } else {
        // default nonsense so things compile
        writeln!(
            out,
            "{}",
            quote::quote! {
            pub(crate) struct Sockets<'a>{
                pub udp: [udp::Socket<'a>; 0],
                pub tcp: [tcp::Socket<'a>; 0]
            }
            pub(crate) fn construct_sockets() -> Sockets<'static> {
                todo!()
            }
                pub(crate) const UDP_SOCKET_PORTS: [u16; 0] = [ ];
            }
        )?;
    }
    writeln!(out, "{}", generate_owner_info(&config)?)?;

    generate_socket_enum(&config, out)?;
    Ok(())
}

fn generate_port_table(
    sockets: &SocketMap,
) -> Result<TokenStream, Box<dyn std::error::Error>> {
    let consts = sockets.values().filter_map(|socket| {
        if socket.kind == "udp" {
            let port = socket.port;
            if port.is_none() {
                panic!("Must specify a port when using udp");
            }
            Some(quote::quote! { #port })
        } else {
            if socket.port.is_some() {
                panic!("Do not specify a port except when using udp");
            }
            None
        }
    });

    let n = sockets.iter().filter(|(_, s)| s.kind == "udp").count();

    Ok(quote::quote! {
        pub(crate) const UDP_SOCKET_PORTS: [u16; #n] = [
            #( #consts ),*
        ];
    })
}

fn generate_owner_info(
    config: &AetherConfig,
) -> Result<TokenStream, Box<dyn std::error::Error>> {
    if let Some(sockets) = &config.sockets {
        let consts = sockets.values().map(|socket| {
            let task: syn::Ident = syn::parse_str(&socket.owner.name).unwrap();
            let note = socket.owner.notification;
            quote::quote! {
                (
                    userlib::TaskId::for_index_and_gen(
                        hubris_num_tasks::Task::#task as usize,
                        userlib::Generation::ZERO,
                    ),
                    #note,
                )
            }
        });

        let n = sockets.len();

        Ok(quote::quote! {
            pub(crate) const SOCKET_OWNERS: [(userlib::TaskId, u32); #n] = [
                #( #consts ),*
            ];
        })
    } else {
        Ok(quote::quote! {
            pub(crate) const SOCKET_OWNERS: [(userlib::TaskId, u32); 0] = [
            ];
        })
    }
}

fn generate_socket_state(
    name: &str,
    config: &SocketConfig,
) -> Result<TokenStream, Box<dyn std::error::Error>> {
    if config.kind == "udp" {
        let tx = generate_udp_buffers(name, "TX", &config.tx)?;
        let rx = generate_udp_buffers(name, "RX", &config.rx)?;
        Ok(quote::quote! {
            #tx
            #rx
        })
    } else if config.kind == "tcp" {
        let tx = generate_tcp_buffers(name, "TX", &config.tx)?;
        let rx = generate_tcp_buffers(name, "RX", &config.rx)?;
        Ok(quote::quote! {
            #tx
            #rx
        })
    } else {
        Err("unsupported socket kind".into())
    }
}

fn generate_udp_buffers(
    name: &str,
    dir: &str,
    config: &BufSize,
) -> Result<TokenStream, Box<dyn std::error::Error>> {
    let pktcnt = config.packets.unwrap();
    let bytecnt = config.bytes;
    let upname = name.to_ascii_uppercase();
    let hdrname: syn::Ident =
        syn::parse_str(&format!("SOCK_UDP_{}_HDR_{}", dir, upname)).unwrap();
    let bufname: syn::Ident =
        syn::parse_str(&format!("SOCK_UDP_{}_DAT_{}", dir, upname)).unwrap();
    Ok(quote::quote! {
        static mut #hdrname: [udp::PacketMetadata; #pktcnt] = [
            udp::PacketMetadata::EMPTY; #pktcnt
        ];
        static mut #bufname: [u8; #bytecnt] = [0u8; #bytecnt];
    })
}

fn generate_tcp_buffers(
    name: &str,
    dir: &str,
    config: &BufSize,
) -> Result<TokenStream, Box<dyn std::error::Error>> {
    assert!(config.packets.is_none());
    let bytecnt = config.bytes;
    let upname = name.to_ascii_uppercase();
    let bufname: syn::Ident =
        syn::parse_str(&format!("SOCK_TCP_{}_{}", dir, upname)).unwrap();
    Ok(quote::quote! {
        static mut #bufname: [u8; #bytecnt] = [0u8; #bytecnt];
    })
}

fn generate_state_struct(
    sockets: &SocketMap,
) -> Result<TokenStream, Box<dyn std::error::Error>> {
    let udp_n = sockets
        .iter()
        .filter(|(_, conf)| conf.kind == "udp")
        .count();
    let tcp_n = sockets
        .iter()
        .filter(|(_, conf)| conf.kind == "tcp")
        .count();
    Ok(quote::quote! {
        pub(crate) struct Sockets<'a>{
            pub udp: [udp::Socket<'a>; #udp_n],
            pub tcp: [tcp::Socket<'a>; #tcp_n]
        }
    })
}

fn generate_constructor(
    sockets: &SocketMap,
) -> Result<TokenStream, Box<dyn std::error::Error>> {
    let udp_sockets = sockets.iter().filter_map(|(name, config)| {
        if config.kind != "udp" {
            return None;
        }
        let upname = name.to_ascii_uppercase();
        let rxhdrs: syn::Ident =
            syn::parse_str(&format!("SOCK_UDP_RX_HDR_{}", upname)).unwrap();
        let rxbytes: syn::Ident =
            syn::parse_str(&format!("SOCK_UDP_RX_DAT_{}", upname)).unwrap();
        let txhdrs: syn::Ident =
            syn::parse_str(&format!("SOCK_UDP_TX_HDR_{}", upname)).unwrap();
        let txbytes: syn::Ident =
            syn::parse_str(&format!("SOCK_UDP_TX_DAT_{}", upname)).unwrap();

        Some(quote::quote! {
            udp::Socket::new(
                udp::PacketBuffer::new(
                    unsafe { &mut #rxhdrs[..] },
                    unsafe { &mut #rxbytes[..] },
                ),
                udp::PacketBuffer::new(
                    unsafe { &mut #txhdrs[..] },
                    unsafe { &mut #txbytes[..] },
                ),
            )
        })
    });
    let tcp_sockets = sockets.iter().filter_map(|(name, config)| {
        if config.kind != "tcp" {
            return None;
        }
        let upname = name.to_ascii_uppercase();
        let rxbytes: syn::Ident =
            syn::parse_str(&format!("SOCK_TCP_RX_{}", upname)).unwrap();
        let txbytes: syn::Ident =
            syn::parse_str(&format!("SOCK_TCP_TX_{}", upname)).unwrap();

        Some(quote::quote! {
            tcp::Socket::new(
                tcp::SocketBuffer::new(
                    unsafe { &mut #rxbytes[..] },
                ),
                tcp::SocketBuffer::new(
                    unsafe { &mut #txbytes[..] },
                ),
            )
        })
    });

    Ok(quote::quote! {
        static CTOR_FLAG: AtomicBool = AtomicBool::new(false);
        pub(crate) fn construct_sockets() -> Sockets<'static> {
            let second_time = CTOR_FLAG.swap(true, Ordering::Relaxed);
            if second_time { panic!() }

            // Now that we're confident we're not aliasing, we can touch these
            // static muts.
            Sockets {
                udp: [ #( #udp_sockets ),* ],
                tcp: [ #( #tcp_sockets ),* ],
            }
        }
    })
}
