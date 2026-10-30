use anyhow::Result;
use clap::{Parser, ValueEnum};
use schemars::schema_for;
use settings::ProjectSettingsContent;
use theme::IconThemeFamilyContent;
use theme_settings::ThemeFamilyContent;

#[derive(Parser, Debug)]
pub struct Args {
    #[arg(value_enum)]
    pub schema_type: SchemaType,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
#[clap(rename_all = "snake_case")]
pub enum SchemaType {
    Theme,
    IconTheme,
    Project,
}

fn main() -> Result<()> {
    env_logger::init();

    let args = Args::parse();

    let schema = match args.schema_type {
        SchemaType::Theme => schema_for!(ThemeFamilyContent),
        SchemaType::IconTheme => schema_for!(IconThemeFamilyContent),
        SchemaType::Project => schema_for!(ProjectSettingsContent),
    };
    println!("{}", serde_json::to_string_pretty(&schema)?);

    Ok(())
}
