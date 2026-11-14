//! A device panel for Android debugging targets.
//!
//! This is a lean adaptation of lathe's `mobile_dev`: it lists the devices
//! `adb` can see and lets the user refresh them. Installing a React Native or
//! Expo build stays a task, so the panel never guesses a package manager or a
//! project layout.
//!
//! Adapted from lathe (https://github.com/paterschris/lathe) at
//! 44d467a53adc884a41e8015f922f00e628841690, licensed GPL-3.0-or-later.

use gpui::{
    Action as _, App, Context, EventEmitter, FocusHandle, Focusable, Pixels, Render, SharedString,
    Window, actions, px,
};
use i18n::tr;
use ui::{
    Button, ButtonSize, ButtonStyle, Color, Icon, IconName, IconSize, Label, LabelSize, prelude::*,
};
use util::{ResultExt as _, command::new_command};
use workspace::{
    Workspace,
    dock::{DockPosition, Panel, PanelEvent},
};

actions!(
    mobile_dev,
    [
        /// Opens or focuses the mobile device panel.
        ToggleMobileDevices,
        /// Re-reads the device list from adb.
        RefreshDevices,
    ]
);

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _window, _cx| {
        workspace.register_action(|workspace, _: &ToggleMobileDevices, window, cx| {
            if !workspace.toggle_panel_focus::<MobileDevicesPanel>(window, cx) {
                open(workspace, window, cx);
            }
        });
        workspace.register_action(|workspace, _: &RefreshDevices, window, cx| {
            match workspace.panel::<MobileDevicesPanel>(cx) {
                Some(panel) => {
                    panel.update(cx, |panel, cx| panel.refresh(cx));
                }
                None => open(workspace, window, cx),
            }
        });
    })
    .detach();
}

fn open(workspace: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
    if workspace.panel::<MobileDevicesPanel>(cx).is_none() {
        let panel = cx.new(|cx| MobileDevicesPanel::new(cx));
        workspace.add_panel(panel, window, cx);
    }
    workspace.focus_panel::<MobileDevicesPanel>(window, cx);
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Device {
    serial: String,
    state: String,
    model: Option<String>,
}

impl Device {
    /// Parses one line of `adb devices -l`. The header and the empty line after
    /// it are skipped.
    fn from_line(line: &str) -> Option<Self> {
        let mut fields = line.split_whitespace();
        let serial = fields.next()?;
        let state = fields.next()?;
        if serial == "List" || serial.starts_with('*') {
            return None;
        }
        let model = fields
            .find_map(|field| field.strip_prefix("model:"))
            .map(str::to_owned);
        Some(Self {
            serial: serial.to_owned(),
            state: state.to_owned(),
            model,
        })
    }

    fn parse(output: &str) -> Vec<Self> {
        output.lines().filter_map(Self::from_line).collect()
    }
}

pub struct MobileDevicesPanel {
    focus_handle: FocusHandle,
    devices: Vec<Device>,
    error: Option<SharedString>,
    loading: bool,
    loaded: bool,
}

impl MobileDevicesPanel {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            devices: Vec::new(),
            error: None,
            loading: false,
            loaded: false,
        }
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.loading = true;
        self.loaded = true;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let mut command = new_command("adb");
            command.args(["devices", "-l"]);
            let result = command.output().await;
            this.update(cx, |this, cx| {
                this.loading = false;
                match result {
                    Ok(output) if output.status.success() => {
                        this.devices = Device::parse(&String::from_utf8_lossy(&output.stdout));
                        this.error = None;
                    }
                    Ok(output) => {
                        this.error = Some(
                            String::from_utf8_lossy(&output.stderr)
                                .trim()
                                .to_owned()
                                .into(),
                        );
                    }
                    Err(error) => {
                        this.error = Some(error.to_string().into());
                    }
                }
                cx.notify();
            })
            .log_err();
        })
        .detach();
    }

    fn render_row(&self, device: &Device, cx: &App) -> impl IntoElement {
        let online = device.state == "device";
        h_flex()
            .justify_between()
            .gap_2()
            .px_2()
            .py_1()
            .border_b_1()
            .border_color(cx.theme().colors().border_variant)
            .child(
                v_flex()
                    .min_w_0()
                    .child(
                        Label::new(
                            device
                                .model
                                .clone()
                                .unwrap_or_else(|| device.serial.clone()),
                        )
                        .size(LabelSize::Small)
                        .truncate(),
                    )
                    .child(
                        Label::new(format!("{} · {}", device.serial, device.state))
                            .size(LabelSize::XSmall)
                            .color(Color::Muted)
                            .truncate(),
                    ),
            )
            .child(
                Icon::new(if online {
                    IconName::Check
                } else {
                    IconName::XCircle
                })
                .size(IconSize::XSmall)
                .color(if online { Color::Success } else { Color::Muted }),
            )
    }
}

impl Render for MobileDevicesPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.loaded {
            self.refresh(cx);
        }

        let content = if let Some(error) = &self.error {
            v_flex()
                .p_2()
                .gap_1()
                .child(
                    Label::new(tr(cx, "mobile_dev.unavailable", "adb is unavailable"))
                        .size(LabelSize::Small)
                        .color(Color::Error),
                )
                .child(
                    Label::new(error.clone())
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                )
                .into_any_element()
        } else if self.devices.is_empty() {
            v_flex()
                .p_2()
                .child(
                    Label::new(tr(cx, "mobile_dev.no_devices", "No devices connected."))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .into_any_element()
        } else {
            let rows: Vec<AnyElement> = self
                .devices
                .iter()
                .map(|device| self.render_row(device, cx).into_any_element())
                .collect();
            v_flex().children(rows).into_any_element()
        };

        v_flex()
            .key_context("MobileDevicesPanel")
            .track_focus(&self.focus_handle)
            .size_full()
            .child(
                h_flex()
                    .justify_between()
                    .px_2()
                    .py_1()
                    .border_b_1()
                    .border_color(cx.theme().colors().border_variant)
                    .child(
                        Label::new(tr(cx, "mobile_dev.title", "Devices"))
                            .size(LabelSize::Small)
                            .weight(gpui::FontWeight::SEMIBOLD),
                    )
                    .child(
                        Button::new(
                            "mobile-dev-refresh",
                            tr(cx, "mobile_dev.refresh", "Refresh"),
                        )
                        .style(ButtonStyle::Subtle)
                        .size(ButtonSize::Compact)
                        .label_size(LabelSize::XSmall)
                        .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                    ),
            )
            .child(
                div()
                    .id("mobile-device-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(content),
            )
    }
}

impl Focusable for MobileDevicesPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for MobileDevicesPanel {}

impl Panel for MobileDevicesPanel {
    fn persistent_name() -> &'static str {
        "MobileDevicesPanel"
    }

    fn panel_key() -> &'static str {
        "MobileDevicesPanel"
    }

    fn position(&self, _window: &Window, _cx: &App) -> DockPosition {
        DockPosition::Right
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        position == DockPosition::Right
    }

    fn set_position(
        &mut self,
        _position: DockPosition,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    fn default_size(&self, _window: &Window, _cx: &App) -> Pixels {
        px(320.)
    }

    fn icon(&self, _window: &Window, _cx: &App) -> Option<IconName> {
        None
    }

    fn icon_tooltip(&self, _window: &Window, cx: &App) -> Option<SharedString> {
        Some(tr(cx, "mobile_dev.title", "Devices").into())
    }

    fn toggle_action(&self) -> Box<dyn gpui::Action> {
        ToggleMobileDevices.boxed_clone()
    }

    fn activation_priority(&self) -> u32 {
        10
    }
}

#[cfg(test)]
mod tests {
    use super::Device;

    #[test]
    fn parses_connected_and_offline_devices() {
        let output = "List of devices attached\n\
            emulator-5554 device product:sdk_gphone64 model:Pixel_7 device:emu64xa transport_id:1\n\
            R58M1234 unauthorized usb:1-1\n";
        let devices = Device::parse(output);
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].serial, "emulator-5554");
        assert_eq!(devices[0].model.as_deref(), Some("Pixel_7"));
        assert_eq!(devices[1].state, "unauthorized");
    }

    #[test]
    fn ignores_the_absent_device_message() {
        assert!(Device::parse("List of devices attached\n\n").is_empty());
    }
}
