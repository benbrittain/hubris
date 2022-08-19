// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use serde::Deserialize;
use std::collections::BTreeMap;
use proc_macro2::TokenStream;
use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    idol::client::build_client_stub("../../../idl/mdns.idol", "client_stub.rs")?;

    Ok(())
}
