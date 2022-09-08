fn main() {
    build_util::expose_target_board();

    let disposition = build_i2c::Disposition::Devices;

    if let Err(e) = build_i2c::codegen(disposition) {
        println!("code generation failed: {}", e);
        std::process::exit(1);
    }
}
