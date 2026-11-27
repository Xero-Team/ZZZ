//! Pins the contract `Settings::from_settings` documents.
//!
//! `SettingsStore::new` calls `from_settings` for every registered setting as
//! soon as the store is built, and `from_settings` panics when
//! `assets/settings/default.json` omits a key the type requires. That makes a
//! missing default a start-up crash for the user and a passing test run for
//! everyone else, because no test named the invariant. Building the store from
//! the shipped defaults is the cheapest way to assert it.

use gpui::TestAppContext;
use settings::SettingsStore;

#[gpui::test]
async fn default_settings_cover_every_registered_setting(cx: &mut TestAppContext) {
    cx.update(|cx| {
        SettingsStore::new(cx, &settings::default_settings());
    });
}
