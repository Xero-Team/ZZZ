fn main() {
    let protoc_path = protoc_bin_vendored::protoc_bin_path().unwrap();

    prost_build::Config::new()
        .protoc_executable(protoc_path)
        .type_attribute("SendDataResponse", "#[allow(clippy::empty_docs)]")
        .compile_protos(
            &["vendored/protocol/livekit_room.proto"],
            &["vendored/protocol"],
        )
        .unwrap();
}
