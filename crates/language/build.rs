fn main() {
    if let Ok(bundled) = std::env::var("ZZZ_BUNDLE") {
        println!("cargo:rustc-env=ZZZ_BUNDLE={}", bundled);
    }
}
