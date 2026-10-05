---
title: Native IME Validation Runbook
description: Repeatable platform steps for EXP-009 TextInputClient conformance.
---

# 原生 IME 验证 runbook

本文固定 EXP-009 的原生平台验证步骤。所有平台都使用只含 `baseline` 的 UTF-8
文本文件；成功结果必须保留该首行，并精确新增 100 行 `你好`，不得丢失、重复或留下
未提交 preedit。Editor 单元层另行覆盖 UTF-16 range、marked text、undo grouping 和
multi-cursor composition。

## Linux X11 + Fcitx5/Rime

前置条件：X11 `DISPLAY`、Fcitx5/Rime、Python `evdev`/`Xlib`，以及当前用户对
`/dev/uinput` 的写权限。脚本只创建临时 virtual keyboard/pointer，窗口、XDG state 和
输入文件都隔离在 `.tmp/gpui-refactor/phase-7/`；退出时销毁 virtual devices 并终止它
启动的 ZZZ 进程。

```sh
cargo build --locked -p zzz
./script/gpui-ime-smoke --binary target/debug/zzz --repetitions 100
```

预期输出包含：

```text
status=PASS
engine=rime
repetitions=100
matching_commits=100
```

脚本同时要求 Fcitx5 candidate window 与 ZZZ editor window 相交。当前 Fedora/X11 主机
结果为 100/100 commits，candidate window `(168, 1928, 560, 152)` 位于 editor window
`(168, 56, 3672, 2104)` 内；证据目录由脚本输出，当前批准结果为
`.tmp/gpui-refactor/phase-7/linux-x11-ime-niz4rous/`。

补充行为检查：

```sh
cargo test --locked -p editor ime
cargo test --locked -p editor focus
cargo test --locked -p editor input
cargo test --locked -p vim test_helix_jump_consumes_label_keystrokes_before_ime
```

## macOS

状态：`NOT RUN`（当前主机不是 macOS）。

1. 启用 `ABC - Extended`、`Pinyin - Simplified` 和一个 RTL input source；给 Terminal 和
   System Events 授予 Accessibility 权限。
2. 构建并用独立 data directory 打开只含 `baseline` 的 `/tmp/zzz-ime-input.txt`：

   ```sh
   cargo build --locked -p zzz
   printf 'baseline\n' > /tmp/zzz-ime-input.txt
   ZZZ_FORCE_CLI_MODE=1 target/debug/zzz \
     --user-data-dir /tmp/zzz-ime-validation \
     /tmp/zzz-ime-input.txt
   ```

3. 聚焦 Editor，切换到 `Pinyin - Simplified`，运行：

   ```sh
   osascript <<'APPLESCRIPT'
   tell application "System Events"
     repeat 100 times
       keystroke "nihao"
       key code 49
       key code 36
       delay 0.05
     end repeat
     keystroke "s" using command down
   end tell
   APPLESCRIPT
   ```

4. 确认 candidate popup 跟随 caret；文件除 `baseline` 外精确包含 100 行 `你好`。
5. 切换 `ABC - Extended`，验证 dead-key `⌥E` 后 `E` 得到 `é`；通过 Character Viewer
   插入 emoji；切换 RTL input source 输入一行 RTL 文本。
6. 建立两个 cursors 后进行一次 composition，确认两处 marked range、commit 和单次 undo
   一致。开启 VoiceOver，仅用键盘聚焦 Editor 后重复一次 composition。

## Windows

状态：`NOT RUN`（当前主机不是 Windows）。

1. 启用 `United States-International`、`Microsoft Pinyin` 和一个 RTL keyboard；准备只含
   `baseline` 的 `%TEMP%\zzz-ime-input.txt`。
2. 在 PowerShell 构建并启动隔离实例：

   ```powershell
   cargo build --locked -p zzz
   Set-Content -Encoding utf8 $env:TEMP\zzz-ime-input.txt "baseline"
   $env:ZZZ_FORCE_CLI_MODE = "1"
   target\debug\zzz.exe --user-data-dir $env:TEMP\zzz-ime-validation `
     $env:TEMP\zzz-ime-input.txt
   ```

3. 聚焦 Editor，切换 `Microsoft Pinyin`，运行：

   ```powershell
   Add-Type -AssemblyName System.Windows.Forms
   1..100 | ForEach-Object {
     [System.Windows.Forms.SendKeys]::SendWait("nihao ")
     [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
     Start-Sleep -Milliseconds 50
   }
   [System.Windows.Forms.SendKeys]::SendWait("^s")
   ```

4. 用 Inspect 确认 candidate popup 跟随 caret；文件除 `baseline` 外精确包含 100 行
   `你好`。
5. 用 `United States-International` 验证 dead key `'+e → é`；用 `Win+.` 插入 emoji；
   切换 RTL keyboard 输入一行 RTL 文本。
6. 建立两个 cursors 后验证 composition/commit/undo；开启 Narrator，仅用键盘聚焦
   Editor 后重复一次 composition。

## Web

Web backend 明确声明 `ime_candidate_position = false`，因此 candidate popup 坐标不作为
静默成功处理。运行：

```sh
cargo check --locked -p gpui_web --target wasm32-unknown-unknown
```

在 browser harness 中验证 compositionstart/update/end 的 committed text、marked range
和 UTF-16 selection；candidate position 的预期结果必须是显式 unsupported，而不是 no-op
success。
