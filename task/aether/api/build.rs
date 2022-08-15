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
    idol::client::build_client_stub("../../../idl/aether.idol", "client_stub.rs")?;

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

    generate_socket_enum(&config, out)?;
    Ok(())
}
