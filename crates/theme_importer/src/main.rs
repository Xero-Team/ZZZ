#[cfg(test)]
mod color;
mod vscode;

use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;

use anyhow::{Context as _, Result};
use clap::Parser;
use collections::IndexMap;
use log::LevelFilter;
use serde::Deserialize;
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use theme::{Appearance, AppearanceContent};

use crate::vscode::{VsCodeTheme, VsCodeThemeConverter};

const ZZZ_THEME_SCHEMA_URL: &str = "https://zed.dev/schema/themes/v0.2.0.json";

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeAppearanceJson {
    Light,
    Dark,
}

impl From<ThemeAppearanceJson> for AppearanceContent {
    fn from(value: ThemeAppearanceJson) -> Self {
        match value {
            ThemeAppearanceJson::Light => Self::Light,
            ThemeAppearanceJson::Dark => Self::Dark,
        }
    }
}

impl From<ThemeAppearanceJson> for Appearance {
    fn from(value: ThemeAppearanceJson) -> Self {
        match value {
            ThemeAppearanceJson::Light => Self::Light,
            ThemeAppearanceJson::Dark => Self::Dark,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ThemeMetadata {
    pub name: String,
    pub file_name: String,
    pub appearance: ThemeAppearanceJson,
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The path to the theme to import.
    theme_path: PathBuf,

    /// Whether to warn when values are missing from the theme.
    #[arg(long)]
    warn_on_missing: bool,

    /// The path to write the output to.
    #[arg(long, short)]
    output: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let log_config = {
        let mut config = simplelog::ConfigBuilder::new();

        if !args.warn_on_missing {
            config.add_filter_ignore_str("theme_printer");
        }

        config.build()
    };

    TermLogger::init(
        LevelFilter::Trace,
        log_config,
        TerminalMode::Stderr,
        ColorChoice::Auto,
    )
    .context("could not initialize logger")?;

    let theme_file_path = args.theme_path;

    let mut buffer = Vec::new();
    File::open(&theme_file_path)
        .and_then(|mut file| file.read_to_end(&mut buffer))
        .with_context(|| format!("failed to read theme file {theme_file_path:?}"))?;

    let vscode_theme: VsCodeTheme = serde_json_lenient::from_slice(&buffer)
        .context(format!("failed to parse theme {theme_file_path:?}"))?;

    let theme_metadata = ThemeMetadata {
        name: vscode_theme.name.clone().unwrap_or_default(),
        appearance: ThemeAppearanceJson::Dark,
        file_name: String::new(),
    };

    let converter = VsCodeThemeConverter::new(vscode_theme, theme_metadata, IndexMap::default());

    let theme = converter.convert();
    let mut theme =
        serde_json::to_value(theme).context("failed to convert theme to a JSON value")?;
    theme
        .as_object_mut()
        .context("serialized theme is not a JSON object")?
        .insert(
            "$schema".to_owned(),
            serde_json::Value::String(ZZZ_THEME_SCHEMA_URL.to_owned()),
        );
    let theme_json =
        serde_json::to_string_pretty(&theme).context("failed to serialize theme to JSON")?;

    if let Some(output) = args.output {
        let mut file = File::create(&output)
            .with_context(|| format!("failed to create output file {output:?}"))?;
        file.write_all(theme_json.as_bytes())
            .with_context(|| format!("failed to write output file {output:?}"))?;
    } else {
        println!("{}", theme_json);
    }

    log::info!("Done!");

    Ok(())
}
