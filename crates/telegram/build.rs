#![allow(clippy::disallowed_methods, reason = "build scripts are exempt")]

use std::path::{Path, PathBuf};

const API_ID_VAR: &str = "ZZZ_TELEGRAM_API_ID";
const API_HASH_VAR: &str = "ZZZ_TELEGRAM_API_HASH";

fn repo_root() -> PathBuf {
    let fallback = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_owned());
    match Path::new(&fallback).parent().and_then(Path::parent) {
        Some(root) => root.to_path_buf(),
        None => PathBuf::from(fallback),
    }
}

fn main() {
    println!("cargo:rerun-if-env-changed={API_ID_VAR}");
    println!("cargo:rerun-if-env-changed={API_HASH_VAR}");
    println!("cargo:rerun-if-changed=credentials.toml");

    let root = repo_root();
    let env_file = root.join(".env");
    println!("cargo:rerun-if-changed={}", env_file.display());

    let mut api_id = std::env::var(API_ID_VAR).ok();
    let mut api_hash = std::env::var(API_HASH_VAR).ok();

    if api_id.is_none() || api_hash.is_none() {
        if let Ok(iter) = dotenvy::from_path_iter(&env_file) {
            for (key, value) in iter.flatten() {
                match key.as_str() {
                    API_ID_VAR if api_id.is_none() => api_id = Some(value),
                    API_HASH_VAR if api_hash.is_none() => api_hash = Some(value),
                    _ => {}
                }
            }
        }
    }

    if api_id.is_none() || api_hash.is_none() {
        if let Some((file_id, file_hash)) = read_credentials_file(&root) {
            api_id = api_id.or(file_id);
            api_hash = api_hash.or(file_hash);
        }
    }

    if let Some(api_id) = api_id {
        println!("cargo:rustc-env={API_ID_VAR}={api_id}");
    }
    if let Some(api_hash) = api_hash {
        println!("cargo:rustc-env={API_HASH_VAR}={api_hash}");
    }
}

fn read_credentials_file(root: &Path) -> Option<(Option<String>, Option<String>)> {
    let path = root.join("crates/telegram/credentials.toml");
    let contents = std::fs::read_to_string(&path).ok()?;
    let value: toml::Value = contents.parse().ok()?;
    let api_id = value.get("api_id").and_then(|value| match value {
        toml::Value::Integer(id) => Some(id.to_string()),
        toml::Value::String(id) => Some(id.clone()),
        _ => None,
    });
    let api_hash = value
        .get("api_hash")
        .and_then(toml::Value::as_str)
        .map(ToOwned::to_owned);
    Some((api_id, api_hash))
}
