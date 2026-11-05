// Propagates the packaging channel into the binary. `cargo xtask bundle`
// exports `ZZZ_RELEASE_CHANNEL` for every target; keeping the rerun directive
// here makes Cargo rebuild this crate when the value changes instead of serving
// a cached `dev` binary from a previous build.
fn main() {
    println!("cargo:rerun-if-env-changed=ZZZ_RELEASE_CHANNEL");
    println!("cargo:rerun-if-changed=../../zzz/RELEASE_CHANNEL");
}
