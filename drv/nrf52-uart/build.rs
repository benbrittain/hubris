
fn main() -> Result<(), Box<dyn std::error::Error>> {
    idol::server::build_server_support(
        "../../idl/nrf52-uart.idol",
        "server_stub.rs",
        idol::server::ServerStyle::Pipelined,
    )?;

    Ok(())
}
