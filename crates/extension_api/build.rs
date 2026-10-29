use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let version = std::env::var("CARGO_PKG_VERSION")?;
    let out_dir = std::env::var("OUT_DIR")?;

    let mut parts = version.split(|c: char| !c.is_ascii_digit());
    let major = version_component(&mut parts, "major")?.to_be_bytes();
    let minor = version_component(&mut parts, "minor")?.to_be_bytes();
    let patch = version_component(&mut parts, "patch")?.to_be_bytes();

    std::fs::write(
        std::path::Path::new(&out_dir).join("version_bytes"),
        [major[0], major[1], minor[0], minor[1], patch[0], patch[1]],
    )?;

    Ok(())
}

/// Reads the next numeric component of `CARGO_PKG_VERSION`.
fn version_component<'a>(
    parts: &mut impl Iterator<Item = &'a str>,
    name: &str,
) -> Result<u16, Box<dyn Error>> {
    let component = parts
        .next()
        .ok_or_else(|| format!("CARGO_PKG_VERSION is missing a {name} component"))?;
    Ok(component.parse()?)
}
