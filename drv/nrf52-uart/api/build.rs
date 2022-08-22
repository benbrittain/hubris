fn main() -> Result<(), Box<dyn std::error::Error>> {
    idol::client::build_client_stub(
        "../../../idl/nrf52-uart.idol",
        "client_stub.rs",
    )?;
    Ok(())
}
