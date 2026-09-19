#![allow(clippy::disallowed_methods, reason = "build scripts are exempt")]

fn main() {
    println!("cargo::rustc-check-cfg=cfg(gles)");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    if target_os == "windows" {
        #[cfg(feature = "windows-manifest")]
        embed_resource();
    }
}

#[cfg(feature = "windows-manifest")]
fn embed_resource() {
    let resource_dir = std::path::Path::new("resources/windows");
    let manifest = resource_dir.join("gpui.manifest.xml");
    let rc_file = resource_dir.join("gpui.rc");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rerun-if-changed={}", rc_file.display());

    #[cfg(windows)]
    embed_resource::compile(rc_file, embed_resource::ParamsIncludeDirs([resource_dir]))
        .manifest_required()
        .unwrap();

    #[cfg(not(windows))]
    {
        let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
        let staged_manifest = out_dir.join("gpui.manifest.xml");
        std::fs::copy(manifest, &staged_manifest).unwrap();
        let staged_rc = out_dir.join("gpui.rc");
        std::fs::write(
            &staged_rc,
            "#define RT_MANIFEST 24\n1 RT_MANIFEST \"gpui.manifest.xml\"\n",
        )
        .unwrap();
        embed_resource::compile(&staged_rc, embed_resource::NONE)
            .manifest_required()
            .unwrap();
    }
}
