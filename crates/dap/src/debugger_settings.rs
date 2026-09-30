use dap_types::SteppingGranularity;
use settings::{RegisterSetting, Settings, SettingsContent};

#[derive(Debug, RegisterSetting)]
pub struct DebuggerSettings {
    /// Determines the stepping granularity.
    ///
    /// Default: line
    pub stepping_granularity: SteppingGranularity,
    /// Whether the breakpoints should be reused across ZZZ sessions.
    ///
    /// Default: true
    pub save_breakpoints: bool,
    /// Whether to show the debug button in the status bar.
    ///
    /// Default: true
    pub button: bool,
    /// Time in milliseconds until timeout error when connecting to a TCP debug adapter
    ///
    /// Default: 2000ms
    pub timeout: u64,
    /// Whether to log messages between active debug adapters and ZZZ
    ///
    /// Default: true
    pub log_dap_communications: bool,
    /// Whether to format dap messages in when adding them to debug adapter logger
    ///
    /// Default: true
    pub format_dap_log_messages: bool,
    /// The dock position of the debug panel
    ///
    /// Default: Bottom
    pub dock: settings::DockPosition,
}

impl Settings for DebuggerSettings {
    fn from_settings(content: &SettingsContent) -> Self {
        let content = content.debugger.clone().expect("value should be present");
        Self {
            stepping_granularity: dap_granularity_from_settings(
                content
                    .stepping_granularity
                    .expect("stepping_granularity should be present"),
            ),
            save_breakpoints: content
                .save_breakpoints
                .expect("save_breakpoints should be present"),
            button: content.button.expect("button should be present"),
            timeout: content.timeout.expect("timeout should be present"),
            log_dap_communications: content
                .log_dap_communications
                .expect("log_dap_communications should be present"),
            format_dap_log_messages: content
                .format_dap_log_messages
                .expect("format_dap_log_messages should be present"),
            dock: content.dock.expect("dock should be present"),
        }
    }
}

fn dap_granularity_from_settings(
    granularity: settings::SteppingGranularity,
) -> dap_types::SteppingGranularity {
    match granularity {
        settings::SteppingGranularity::Instruction => dap_types::SteppingGranularity::Instruction,
        settings::SteppingGranularity::Line => dap_types::SteppingGranularity::Line,
        settings::SteppingGranularity::Statement => dap_types::SteppingGranularity::Statement,
    }
}
