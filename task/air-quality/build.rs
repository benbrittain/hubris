use std::{env, path::PathBuf};

fn main() {
    build_util::expose_target_board();

    bsec();

    let disposition = build_i2c::Disposition::Devices;

    if let Err(e) = build_i2c::codegen(disposition) {
        println!("code generation failed: {}", e);
        std::process::exit(1);
    }
}

fn bsec() {
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("bsec.rs");

    let bsec_include_path = PathBuf::from("./bsec-generic/algo/normal_version/inc/");
    let bsec_library_path =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("./bsec-generic/algo/normal_version/bin/gcc/Cortex_M4F/");

    println!("cargo:rerun-if-changed={}", bsec_include_path.display());
    println!(
        "cargo:rustc-link-search=native={}",
        bsec_library_path.display()
    );
    println!("cargo:rustc-link-lib=static=algobsec");

    let bindings = bindgen::Builder::default()
        .header(
            PathBuf::from(bsec_include_path)
                .join("bsec_interface.h")
                .to_str()
                .unwrap(),
        )
        .use_core()
        .ctypes_prefix("cty")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .generate()
        .expect("Unable to generate BSEC bindings.");
    bindings.write_to_file(out_path).unwrap()
}
