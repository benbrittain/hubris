fn main() -> Result<(), Box<dyn std::error::Error>> {
    idol::client::build_client_stub("../../../idl/aether.idol", "client_stub.rs")?;
    Ok(())
}
