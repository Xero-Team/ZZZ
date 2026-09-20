fn main() {
    println!("cargo:rerun-if-changed=proto");
    let protoc_path = protoc_bin_vendored::protoc_bin_path().unwrap();

    let mut build = prost_build::Config::new();
    build
        .protoc_executable(protoc_path)
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .compile_protos(&["proto/zzz.proto"], &["proto"])
        .unwrap();
}
