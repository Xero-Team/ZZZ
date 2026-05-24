[CmdletBinding()]
param(
    [Parameter()][Alias('a')][string]$Architecture = 'x86_64',
    [Parameter()][string]$ReleaseDir,
    [Parameter()][string]$Output,
    [Parameter()][string]$Channel,
    [Parameter()][string]$Version,
    [Parameter()][string]$WorkspaceRoot,
    [Parameter()][switch]$DesktopShortcut,
    [Parameter()][Alias('h')][switch]$Help
)

$ErrorActionPreference = 'Stop'

if ($Help) {
    Write-Output 'Usage: bundle-windows-msi.ps1 [-Architecture x86_64|aarch64] [-ReleaseDir path] [-Output path] [-Channel name] [-Version x.y.z] [-WorkspaceRoot path] [-DesktopShortcut]'
    exit 0
}

if (-not $WorkspaceRoot) {
    $WorkspaceRoot = Split-Path -Parent $PSScriptRoot
}

$projectPath = Join-Path $WorkspaceRoot 'tooling/windows_msi_builder/WindowsMsiBuilder.csproj'
if (-not (Test-Path $projectPath)) {
    throw "MSI builder project was not found: $projectPath"
}

$arguments = @('run', '--project', $projectPath, '--')

if ($Architecture) {
    $arguments += @('--arch', $Architecture)
}

if ($WorkspaceRoot) {
    $arguments += @('--workspace-root', $WorkspaceRoot)
}

if ($ReleaseDir) {
    $arguments += @('--release-dir', $ReleaseDir)
}

if ($Output) {
    $arguments += @('--output', $Output)
}

if ($Channel) {
    $arguments += @('--channel', $Channel)
}

if ($Version) {
    $arguments += @('--version', $Version)
}

if ($DesktopShortcut) {
    $arguments += '--desktop-shortcut'
}

& dotnet @arguments
if ($LASTEXITCODE -ne 0) {
    throw "MSI bundling failed with exit code $LASTEXITCODE"
}