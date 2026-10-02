# Static survey: gpui-mobile

- Checkout: `/home/begonia/Documents/Github/XeroTeam/ZZZ/.tmp/ui_ref/gpui-mobile`
- HEAD: `c7cab3a43970bd5f1e05d907695404ed73fcdc95`
- Tracked files: 261

## Top-level subsystems

- `src`: 161 files
- `example`: 83 files
- `screenshots`: 5 files
- `.github`: 3 files
- `.cargo`: 1 files
- `.gitignore`: 1 files
- `Cargo.toml`: 1 files
- `LICENSE-AGPL`: 1 files
- `LICENSE-APACHE`: 1 files
- `LICENSE-GPL`: 1 files
- `PACKAGES-TO-IMPL.md`: 1 files
- `README.md`: 1 files
- `TODO.md`: 1 files

## Manifests

- `Cargo.toml`
- `example/Cargo.toml`

## Candidate entrypoints

- `example/src/lib.rs`
- `example/src/main.rs`
- `src/lib.rs`

## Tests, benchmarks, and evals

- None detected by the static survey.

## Architecture and policy documents

- `README.md`
- `example/README.md`

## Declared dependencies

- `anyhow`
- `futures`
- `gpui`
- `gpui-mobile`
- `log`
- `parking_lot`
- `raw-window-handle`
- `reqwest`
- `reqwest_client`

## Architecture keyword hints

- **agent**: `src/packages/webview/mod.rs`
- **browser**: `example/src/screens/mod.rs`, `example/src/screens/packages_demo.rs`, `example/src/screens/webview_browser.rs`, `src/packages/url_launcher/mod.rs`
- **cache**: `.github/workflows/ci.yml`, `.github/workflows/publish.yml`, `example/android/gradle/app/src/main/java/dev/gpui/mobile/GpuiCamera.java`, `example/android/gradle/app/src/main/java/dev/gpui/mobile/GpuiLocation.java`, `example/android/gradle/app/src/main/java/dev/gpui/mobile/GpuiMicrophone.java`, `example/src/screens/packages_demo.rs`, `src/android/jni.rs`, `src/android/window.rs`, `src/packages/path_provider/android.rs`, `src/packages/path_provider/ios.rs`, `src/packages/path_provider/mod.rs`, `src/packages/sensors/ios.rs`
- **embed**: `PACKAGES-TO-IMPL.md`, `example/android/gradle/app/src/main/java/dev/gpui/mobile/GpuiPlatformView.java`, `example/src/screens/chat.rs`, `src/android/platform_view.rs`, `src/components/material/fab.rs`, `src/components/platform_view_element.rs`, `src/ios/platform_view.rs`, `src/ios/text_system.rs`, `src/packages/maps/mod.rs`, `src/packages/video_player/mod.rs`, `src/packages/webview/mod.rs`, `src/platform_view.rs`
- **evaluation**: `src/packages/webview/ios.rs`
- **graph**: `Cargo.toml`, `example/android/gradle/app/src/main/java/dev/gpui/mobile/GpuiCamera.java`, `example/android/gradle/app/src/main/java/dev/gpui/mobile/GpuiHelper.java`, `example/ios/project.yml`, `example/src/screens/about.rs`, `example/src/screens/swiper.rs`, `src/components/material/mod.rs`, `src/components/material/theme.rs`, `src/components/shared/skeleton.rs`, `src/ios/cg_types.rs`, `src/ios/text_system.rs`, `src/ios/window.rs`
- **index**: `example/android/gradle/app/src/main/java/dev/gpui/mobile/GpuiPlatformView.java`, `example/src/screens/audio_player.rs`, `example/src/screens/swiper.rs`, `example/src/screens/video_player.rs`, `src/android/jni.rs`, `src/android/platform_view.rs`, `src/android/window.rs`, `src/components/material/card.rs`, `src/components/material/controls.rs`, `src/components/material/navigation_bar.rs`, `src/components/material/navigation_drawer.rs`, `src/components/material/navigation_rail.rs`
- **provider**: `Cargo.toml`, `example/android/gradle/app/src/main/java/dev/gpui/mobile/GpuiCalendar.java`, `example/android/gradle/app/src/main/java/dev/gpui/mobile/GpuiContacts.java`, `example/android/gradle/app/src/main/java/dev/gpui/mobile/GpuiFilePicker.java`, `example/android/gradle/app/src/main/java/dev/gpui/mobile/GpuiImagePicker.java`, `example/android/gradle/app/src/main/java/dev/gpui/mobile/GpuiLocation.java`, `example/android/gradle/app/src/main/java/dev/gpui/mobile/GpuiPermissions.java`, `example/src/screens/packages_demo.rs`, `src/ios/text_system.rs`, `src/packages/image_picker/ios.rs`, `src/packages/location/mod.rs`, `src/packages/mod.rs`
- **queue**: `src/android/dispatcher.rs`, `src/android/jni.rs`, `src/android/mod.rs`, `src/android/platform.rs`, `src/android/window.rs`, `src/ios/dispatcher.rs`, `src/ios/window.rs`
- **rank**: `.cargo/config.toml`, `Cargo.toml`
- **retriev**: `example/android/gradle/app/src/main/java/dev/gpui/mobile/GpuiPickerActivity.java`, `src/android/platform.rs`, `src/ios/ffi.rs`, `src/ios/text_system.rs`, `src/packages/device_info/mod.rs`, `src/packages/network_info/mod.rs`, `src/packages/package_info/mod.rs`
- **search**: `example/android/gradle/app/src/main/java/dev/gpui/mobile/GpuiContacts.java`, `example/ios/project.yml`, `example/src/screens/components.rs`, `src/components/glass/mod.rs`, `src/components/glass/search_bar.rs`, `src/components/glass/tab_bar.rs`, `src/components/material/app_bar.rs`, `src/components/material/list_tile.rs`, `src/components/material/mod.rs`, `src/components/material/navigation_rail.rs`, `src/components/material/scaffold.rs`, `src/components/material/search_bar.rs`
- **symbol**: `Cargo.toml`, `example/README.md`, `example/android/gradle/app/src/main/java/dev/gpui/mobile/GpuiActivity.java`, `example/src/lib.rs`, `src/android/dispatcher.rs`, `src/android/keyboard.rs`, `src/android/window.rs`, `src/ios/text_system.rs`
- **vector**: `src/ios/text_system.rs`

## Interpretation limit

This survey identifies candidate files. It does not establish behavior; read and cite the implementation and tests before making claims.
