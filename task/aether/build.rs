// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// almost directly copied from the build_net stuff, maybe integrate back
// in at some point

use serde::Deserialize;
use std::collections::BTreeMap;
use proc_macro2::TokenStream;
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
    pub port: u16,
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
    pub packets: usize,
    pub bytes: usize,
}


#[derive(Deserialize)]
pub struct GlobalConfig {
    pub aether: AetherConfig,
}

#[derive(Deserialize)]
pub struct AetherConfig {
    pub pan_id: u16,
    /// Sockets known to the system, indexed by name.
    pub sockets: BTreeMap<String, SocketConfig>,
}

pub fn load_aether_config() -> Result<AetherConfig, Box<dyn std::error::Error>>
{
    Ok(build_util::config::<GlobalConfig>()?.aether)
}

pub fn generate_socket_enum(
    config: &AetherConfig,
    mut out: impl std::io::Write,
) -> Result<(), std::io::Error> {
    writeln!(out, "#[allow(non_camel_case_types)]")?;
    writeln!(out, "#[repr(u8)]")?;
    writeln!(
        out,
        "#[derive(Copy, Clone, Debug, Eq, PartialEq, userlib::FromPrimitive)]"
    )?;
    writeln!(out, "#[derive(serde::Serialize, serde::Deserialize)]")?;
    writeln!(out, "pub enum SocketName {{")?;
    for (i, name) in config.sockets.keys().enumerate() {
        writeln!(out, "    {} = {},", name, i)?;
    }
    writeln!(out, "}}")?;
    Ok(())
}

fn generate_aether_config(
    config: &AetherConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = std::env::var("OUT_DIR")?;
    let dest_path = std::path::Path::new(&out_dir).join("aether_config.rs");

    let mut out = std::fs::File::create(&dest_path)?;

    let socket_count = config.sockets.len();
    writeln!(
        out,
        "{}",
        quote::quote! {
            use core::sync::atomic::{AtomicBool, Ordering};
            use smoltcp::socket::{UdpPacketMetadata, UdpSocket, UdpSocketBuffer};

            pub const SOCKET_COUNT: usize = #socket_count;
        }
    )?;

    for (name, socket) in &config.sockets {
        writeln!(out, "{}", generate_socket_state(name, socket)?)?;
    }

    let pan_id = config.pan_id;
    writeln!(out, "{}",
        quote::quote! {
            use smoltcp::wire::Ieee802154Pan;
            pub const PAN_ID: Ieee802154Pan = Ieee802154Pan(#pan_id);
        }
    )?;

    writeln!(out, "{}", generate_state_struct(&config)?)?;
    writeln!(out, "{}", generate_constructor(&config)?)?;
    writeln!(out, "{}", generate_owner_info(&config)?)?;
    writeln!(out, "{}", generate_port_table(&config)?)?;

    generate_socket_enum(&config, out)?;
    Ok(())
}

fn generate_port_table(
    config: &AetherConfig,
) -> Result<TokenStream, Box<dyn std::error::Error>> {
    let consts = config.sockets.values().map(|socket| {
        let port = socket.port;
        quote::quote! { #port }
    });

    let n = config.sockets.len();

    Ok(quote::quote! {
        pub(crate) const SOCKET_PORTS: [u16; #n] = [
            #( #consts ),*
        ];
    })
}

fn generate_owner_info(
    config: &AetherConfig,
) -> Result<TokenStream, Box<dyn std::error::Error>> {
    let consts = config.sockets.values().map(|socket| {
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

    let n = config.sockets.len();

    Ok(quote::quote! {
        pub(crate) const SOCKET_OWNERS: [(userlib::TaskId, u32); #n] = [
            #( #consts ),*
        ];
    })
}

fn generate_socket_state(
    name: &str,
    config: &SocketConfig,
) -> Result<TokenStream, Box<dyn std::error::Error>> {
    if config.kind != "udp" {
        return Err("unsupported socket kind".into());
    }

    let tx = generate_buffers(name, "TX", &config.tx)?;
    let rx = generate_buffers(name, "RX", &config.rx)?;
    Ok(quote::quote! {
        #tx
        #rx
    })
}

fn generate_buffers(
    name: &str,
    dir: &str,
    config: &BufSize,
) -> Result<TokenStream, Box<dyn std::error::Error>> {
    let pktcnt = config.packets;
    let bytecnt = config.bytes;
    let upname = name.to_ascii_uppercase();
    let hdrname: syn::Ident =
        syn::parse_str(&format!("SOCK_{}_HDR_{}", dir, upname)).unwrap();
    let bufname: syn::Ident =
        syn::parse_str(&format!("SOCK_{}_DAT_{}", dir, upname)).unwrap();
    Ok(quote::quote! {
        static mut #hdrname: [UdpPacketMetadata; #pktcnt] = [
            UdpPacketMetadata::EMPTY; #pktcnt
        ];
        static mut #bufname: [u8; #bytecnt] = [0u8; #bytecnt];
    })
}

fn generate_state_struct(
    config: &AetherConfig,
) -> Result<TokenStream, Box<dyn std::error::Error>> {
    let n = config.sockets.len();
    Ok(quote::quote! {
        pub(crate) struct Sockets<'a>(pub [UdpSocket<'a>; #n]);
    })
}

fn generate_constructor(
    config: &AetherConfig,
) -> Result<TokenStream, Box<dyn std::error::Error>> {
    let sockets = config.sockets.keys().map(|name| {
        let upname = name.to_ascii_uppercase();
        let rxhdrs: syn::Ident =
            syn::parse_str(&format!("SOCK_RX_HDR_{}", upname)).unwrap();
        let rxbytes: syn::Ident =
            syn::parse_str(&format!("SOCK_RX_DAT_{}", upname)).unwrap();
        let txhdrs: syn::Ident =
            syn::parse_str(&format!("SOCK_TX_HDR_{}", upname)).unwrap();
        let txbytes: syn::Ident =
            syn::parse_str(&format!("SOCK_TX_DAT_{}", upname)).unwrap();

        quote::quote! {
            UdpSocket::new(
                UdpSocketBuffer::new(
                    unsafe { &mut #rxhdrs[..] },
                    unsafe { &mut #rxbytes[..] },
                ),
                UdpSocketBuffer::new(
                    unsafe { &mut #txhdrs[..] },
                    unsafe { &mut #txbytes[..] },
                ),
            )
        }
    });
    Ok(quote::quote! {
        static CTOR_FLAG: AtomicBool = AtomicBool::new(false);
        pub(crate) fn construct_sockets() -> Sockets<'static> {
            let second_time = CTOR_FLAG.swap(true, Ordering::Relaxed);
            if second_time { panic!() }

            // Now that we're confident we're not aliasing, we can touch these
            // static muts.
            Sockets([
                #( #sockets ),*
            ])
        }
    })
}