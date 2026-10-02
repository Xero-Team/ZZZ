$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$global:PSNativeCommandUseErrorActionPreference = $false

$CARGO_ABOUT_VERSION="0.8.2"
$outputFile = if ($args.Count -gt 0 -and $args[0]) {
    $args[0]
} else {
    "$(Get-Location)/assets/licenses.md"
}
$templateFile="script/licenses/template.md.hbs"

New-Item -Path "$outputFile" -ItemType File -Value "" -Force

@(
    "# ###### THEME LICENSES ######\n"
    Get-Content assets/themes/LICENSES
    "\n# ###### ICON LICENSES ######\n"
    Get-Content assets/icons/LICENSES
    "\n# ###### CODE LICENSES ######\n"
) | Add-Content -Path $outputFile

$needsInstall = $false
try {
    $versionOutput = cargo about --version
    if (-not ($versionOutput -match "cargo-about $CARGO_ABOUT_VERSION")) {
        $needsInstall = $true
    } else {
        Write-Host "cargo-about@$CARGO_ABOUT_VERSION is already installed"
    }
} catch {
    $needsInstall = $true
}

if ($needsInstall) {
    Write-Host "Installing cargo-about@$CARGO_ABOUT_VERSION..."
    cargo install "cargo-about@$CARGO_ABOUT_VERSION"
}

Write-Host "Generating cargo licenses"

$failFlag = if ($env:ALLOW_MISSING_LICENSES) {
    "--fail"
} else {
    ""
}
$args = @('about', 'generate', $failFlag, '-c', 'script/licenses/zzz-licenses.toml', $templateFile, '-o', $outputFile) | Where-Object { $_ }
cargo @args

Write-Host "Applying replacements"
$replacements = @{
    '&quot;' = '"'
    '&#x27;' = "'"
    '&#x3D;' = '='
    '&#x60;' = '`'
    '&lt;'   = '<'
    '&gt;'   = '>'
}
$content = Get-Content $outputFile
foreach ($find in $replacements.keys) {
    $content = $content -replace $find, $replacements[$find]
}
$content | Set-Content $outputFile

@(
    "`n# ###### NATIVE LIBRARY LICENSES ######`n"
    "#### GNU Lesser General Public License version 3"
    ""
    "##### Used by:"
    ""
    "* [FFmpeg 9.0.2-22-g46d8f462ee](https://github.com/FFmpeg/FFmpeg/commit/46d8f462ee)"
    "* Prebuilt by [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds/tree/autobuild-2026-10-01-13-06)"
    ""
    Get-Content script/licenses/ffmpeg-LICENSE.txt
) | Add-Content -Path $outputFile

Write-Host "generate-licenses completed. See $outputFile"
