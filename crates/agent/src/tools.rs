mod context_server_registry;

pub use context_server_registry::*;

pub struct TerminalTool;

impl TerminalTool {
    pub const NAME: &'static str = "terminal";
}

pub const ALL_TOOL_NAMES: &[&str] = &[
    "copy_path",
    "create_directory",
    "delete_path",
    "edit_file",
    "fetch",
    "move_path",
    "restore_file_from_disk",
    "save_file",
    "skill",
    "terminal",
];

pub fn tool_supports_provider(
    _name: &str,
    _provider: &language_model::LanguageModelProviderId,
) -> bool {
    true
}
