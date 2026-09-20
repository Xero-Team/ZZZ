[CmdletBinding()]
Param(
    [Parameter()][Alias('i')][switch]$Install,
    [Parameter()][Alias('h')][switch]$Help,
    [Parameter()][Alias('a')][string]$Architecture,
    [Parameter()][string]$Name
)

. "$PSScriptRoot/lib/workspace.ps1"

# https://stackoverflow.com/questions/57949031/powershell-script-stops-if-program-fails-like-bash-set-o-errexit
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$global:PSNativeCommandUseErrorActionPreference = $false

$buildSuccess = $false

function Invoke-NativeCommand {
    param(
        [Parameter(Mandatory)][string]$FilePath,
        [string[]]$ArgumentList = @(),
        [Parameter(Mandatory)][string]$Description
    )

    $standardOutputPath = [System.IO.Path]::GetTempFileName()
    $standardErrorPath = [System.IO.Path]::GetTempFileName()

    try {
        $process = Start-Process -FilePath $FilePath -ArgumentList $ArgumentList -NoNewWindow -Wait -PassThru -RedirectStandardOutput $standardOutputPath -RedirectStandardError $standardErrorPath

        if ((Get-Item $standardOutputPath).Length -gt 0) {
            Get-Content $standardOutputPath
        }

        if ((Get-Item $standardErrorPath).Length -gt 0) {
            Get-Content $standardErrorPath
        }

        if ($process.ExitCode -ne 0) {
            throw "$Description failed with exit code $($process.ExitCode)"
        }
    }
    finally {
        Remove-Item $standardOutputPath -ErrorAction SilentlyContinue
        Remove-Item $standardErrorPath -ErrorAction SilentlyContinue
    }
}

function Invoke-NativeCommandWithOutput {
    param(
        [Parameter(Mandatory)][string]$FilePath,
        [string[]]$ArgumentList = @(),
        [Parameter(Mandatory)][string]$Description
    )

    $standardOutputPath = [System.IO.Path]::GetTempFileName()
    $standardErrorPath = [System.IO.Path]::GetTempFileName()

    try {
        $process = Start-Process -FilePath $FilePath -ArgumentList $ArgumentList -NoNewWindow -Wait -PassThru -RedirectStandardOutput $standardOutputPath -RedirectStandardError $standardErrorPath
        $standardError = Get-Content $standardErrorPath -ErrorAction SilentlyContinue

        if ($standardError) {
            Write-Output $standardError
        }

        if ($process.ExitCode -ne 0) {
            throw "$Description failed with exit code $($process.ExitCode)"
        }

        return Get-Content $standardOutputPath -ErrorAction SilentlyContinue
    }
    finally {
        Remove-Item $standardOutputPath -ErrorAction SilentlyContinue
        Remove-Item $standardErrorPath -ErrorAction SilentlyContinue
    }
}

function Get-ArchitectureTriple {
    param(
        [AllowNull()][string]$Architecture
    )

    switch ($Architecture) {
        "X64" { "x86_64" }
        "AMD64" { "x86_64" }
        "Arm64" { "aarch64" }
        "ARM64" { "aarch64" }
        default {
            if ($Architecture) {
                throw "Unsupported architecture: $Architecture"
            }

            throw "Unable to determine operating system architecture"
        }
    }
}

$runtimeArchitecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
$runtimeArchitectureName = if ($null -ne $runtimeArchitecture) {
    $runtimeArchitecture.ToString()
} else {
    $null
}

$detectedArchitecture = if ($runtimeArchitectureName) {
    $runtimeArchitectureName
} elseif ($env:PROCESSOR_ARCHITEW6432) {
    $env:PROCESSOR_ARCHITEW6432
} else {
    $env:PROCESSOR_ARCHITECTURE
}

$OSArchitecture = Get-ArchitectureTriple -Architecture $detectedArchitecture

$Architecture = if ($Architecture) {
    $Architecture
} else {
    $OSArchitecture
}

if (-not $env:CARGO_HOME -and $env:CI -and $env:RUNNER_TEMP) {
    $env:CARGO_HOME = Join-Path $env:RUNNER_TEMP 'cargo'
}

if ($env:CARGO_HOME) {
    New-Item -Path $env:CARGO_HOME -ItemType Directory -Force | Out-Null
    $env:Path = "$env:CARGO_HOME\bin;$env:Path"
}

$CargoOutDir = "./target/$Architecture-pc-windows-msvc/release"
$CargoBuildJobs = if ($env:ZZZ_WINDOWS_BUNDLE_JOBS) {
    $env:ZZZ_WINDOWS_BUNDLE_JOBS
} elseif (-not $env:CI) {
    '32'
} else {
    $null
}

function Get-CargoBuildArguments {
    param(
        [Parameter(Mandatory)][string[]]$Arguments
    )

    if ($CargoBuildJobs) {
        return $Arguments + @('--jobs', $CargoBuildJobs)
    }

    return $Arguments
}

function Get-VSArch {
    param(
        [string]$Arch
    )

    switch ($Arch) {
        "x86_64" { "amd64" }
        "aarch64" { "arm64" }
        default { throw "Unsupported Visual Studio architecture: $Arch" }
    }
}

function Get-VSDevShellPath {
    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"

    if (-not (Test-Path $vswhere)) {
        throw "Unable to locate vswhere.exe at $vswhere"
    }

    $installationPath = Invoke-NativeCommandWithOutput -FilePath $vswhere -ArgumentList @(
        '-latest',
        '-products',
        '*',
        '-requires',
        'Microsoft.VisualStudio.Component.VC.Tools.x86.x64',
        '-property',
        'installationPath'
    ) -Description "Locate Visual Studio installation"

    if (-not $installationPath) {
        throw "Unable to locate a Visual Studio installation with C++ build tools"
    }

    $devShellPath = Join-Path $installationPath "Common7\Tools\Launch-VsDevShell.ps1"

    if (-not (Test-Path $devShellPath)) {
        throw "Unable to locate Launch-VsDevShell.ps1 at $devShellPath"
    }

    return $devShellPath
}

Push-Location
if (-not $env:VSCMD_VER) {
    & (Get-VSDevShellPath) -Arch (Get-VSArch -Arch $Architecture) -HostArch (Get-VSArch -Arch $OSArchitecture)
}
Pop-Location

$target = "$Architecture-pc-windows-msvc"

if ($Help) {
    Write-Output "Usage: test.ps1 [-Install] [-Help]"
    Write-Output "Build the installer for Windows.\n"
    Write-Output "Options:"
    Write-Output "  -Architecture, -a Which architecture to build (x86_64 or aarch64)"
    Write-Output "  -Install, -i      Run the installer after building."
    Write-Output "  -Help, -h         Show this help message."
    exit 0
}

Push-Location -Path crates/zzz
$channel = Get-Content "RELEASE_CHANNEL"
$env:ZZZ_RELEASE_CHANNEL = $channel
$env:RELEASE_CHANNEL = $channel
Pop-Location

function CheckEnvironmentVariables {
    if(-not $env:CI) {
        return
    }

    $requiredVars = @(
        'ZZZ_WORKSPACE', 'RELEASE_VERSION', 'ZZZ_RELEASE_CHANNEL',
        'AZURE_TENANT_ID', 'AZURE_CLIENT_ID', 'AZURE_CLIENT_SECRET',
        'ACCOUNT_NAME', 'CERT_PROFILE_NAME', 'ENDPOINT',
        'FILE_DIGEST', 'TIMESTAMP_DIGEST', 'TIMESTAMP_SERVER'
    )

    foreach ($var in $requiredVars) {
        if (-not (Test-Path "env:$var")) {
            Write-Error "$var is not set"
            exit 1
        }
    }
}

function PrepareForBundle {
    if (Test-Path "$innoDir") {
        Remove-Item -Path "$innoDir" -Recurse -Force
    }
    New-Item -Path "$innoDir" -ItemType Directory -Force
    Copy-Item -Path "$env:ZZZ_WORKSPACE\crates\zzz\resources\windows\*" -Destination "$innoDir" -Recurse -Force
    New-Item -Path "$innoDir\make_appx" -ItemType Directory -Force
    New-Item -Path "$innoDir\appx" -ItemType Directory -Force
    New-Item -Path "$innoDir\bin" -ItemType Directory -Force
    New-Item -Path "$innoDir\tools" -ItemType Directory -Force

    Invoke-NativeCommand -FilePath 'rustup' -ArgumentList @('target', 'add', $target) -Description "Add rustup target $target"
}

function GenerateLicenses {
    . $PSScriptRoot/generate-licenses.ps1
}

function BuildZZZAndItsFriends {
    Write-Output "Building ZZZ and its friends, for channel: $channel"
    # Build zzz.exe and cli.exe after remote_server so the archive can be embedded.
    $env:ZZZ_EMBED_REMOTE_SERVER_DIR = Join-Path $env:ZZZ_WORKSPACE 'target'
    Invoke-NativeCommand -FilePath 'cargo' -ArgumentList (Get-CargoBuildArguments -Arguments @('build', '--release', '--package', 'zzz', '--package', 'cli', '--target', $target)) -Description "Build zzz and cli for $target"
    Copy-Item -Path ".\$CargoOutDir\zzz.exe" -Destination "$innoDir\ZZZ.exe" -Force
    Copy-Item -Path ".\$CargoOutDir\cli.exe" -Destination "$innoDir\cli.exe" -Force
    # Build explorer_command_injector.dll
    switch ($channel) {
        "stable" {
            Invoke-NativeCommand -FilePath 'cargo' -ArgumentList (Get-CargoBuildArguments -Arguments @('build', '--release', '--features', 'stable', '--no-default-features', '--package', 'explorer_command_injector', '--target', $target)) -Description "Build explorer_command_injector for stable $target"
        }
        "preview" {
            Invoke-NativeCommand -FilePath 'cargo' -ArgumentList (Get-CargoBuildArguments -Arguments @('build', '--release', '--features', 'preview', '--no-default-features', '--package', 'explorer_command_injector', '--target', $target)) -Description "Build explorer_command_injector for preview $target"
        }
        default {
            Invoke-NativeCommand -FilePath 'cargo' -ArgumentList (Get-CargoBuildArguments -Arguments @('build', '--release', '--package', 'explorer_command_injector', '--target', $target)) -Description "Build explorer_command_injector for $target"
        }
    }
    Copy-Item -Path ".\$CargoOutDir\explorer_command_injector.dll" -Destination "$innoDir\zzz_explorer_command_injector.dll" -Force
}

function BuildRemoteServer {
    Write-Output "Building remote_server for $target"
    $env:ZZZ_BUILDING_REMOTE_SERVER = '1'
    try {
        Invoke-NativeCommand -FilePath 'cargo' -ArgumentList (Get-CargoBuildArguments -Arguments @('build', '--release', '--package', 'remote_server', '--target', $target)) -Description "Build remote_server for $target"
    }
    finally {
        Remove-Item Env:ZZZ_BUILDING_REMOTE_SERVER -ErrorAction SilentlyContinue
    }

    # Create zipped remote server binary
    $remoteServerSrc = (Resolve-Path ".\$CargoOutDir\remote_server.exe").Path

    if ($env:CI) {
        Write-Output "Code signing remote_server.exe"
        & "$innoDir\sign.ps1" $remoteServerSrc
    }

    $remoteServerDst = "$env:ZZZ_WORKSPACE\target\zzz-remote-server-windows-$Architecture.zip"
    Write-Output "Compressing remote_server to $remoteServerDst"
    Compress-Archive -Path $remoteServerSrc -DestinationPath $remoteServerDst -Force

    Write-Output "Remote server compressed successfully"
}

function ZipZZZAndItsFriendsDebug {
    $items = @(
        ".\$CargoOutDir\zzz.pdb",
        ".\$CargoOutDir\cli.pdb",
        ".\$CargoOutDir\explorer_command_injector.pdb",
        ".\$CargoOutDir\remote_server.pdb"
    )

    Compress-Archive -Path $items -DestinationPath ".\$CargoOutDir\zzz-$env:RELEASE_VERSION-$env:ZZZ_RELEASE_CHANNEL.dbg.zip" -Force
}


function UploadToSentry {
    if ($env:ZZZ_ENABLE_SENTRY_UPLOAD -ne "1") {
        Write-Output "ZZZ_ENABLE_SENTRY_UPLOAD is not set to 1; skipped sentry upload."
        return
    }
    if (-not (Get-Command "sentry-cli" -ErrorAction SilentlyContinue)) {
        Write-Output "sentry-cli not found. skipping sentry upload."
        Write-Output "install with: 'winget install -e --id=Sentry.sentry-cli'"
        return
    }
    if ([string]::IsNullOrWhiteSpace($env:SENTRY_AUTH_TOKEN)) {
        Write-Output "SENTRY_AUTH_TOKEN is missing; skipped sentry upload."
        return
    }
    Write-Output "Uploading zzz debug symbols to sentry..."
    for ($i = 1; $i -le 3; $i++) {
        try {
            sentry-cli debug-files upload --include-sources --wait -p zed -o zed-dev $CargoOutDir
            break
        }
        catch {
            Write-Output "Sentry upload attempt $i failed: $_"
            if ($i -eq 3) {
                Write-Output "All sentry upload attempts failed"
                throw
            }
            Start-Sleep -Seconds 2
        }
    }
}

function MakeAppx {
    switch ($channel) {
        "stable" {
            $manifestFile = "$env:ZZZ_WORKSPACE\crates\explorer_command_injector\AppxManifest.xml"
        }
        "preview" {
            $manifestFile = "$env:ZZZ_WORKSPACE\crates\explorer_command_injector\AppxManifest-Preview.xml"
        }
        default {
            $manifestFile = "$env:ZZZ_WORKSPACE\crates\explorer_command_injector\AppxManifest-Nightly.xml"
        }
    }
    Copy-Item -Path "$manifestFile" -Destination "$innoDir\make_appx\AppxManifest.xml"
    # Add makeAppx.exe to Path
    $sdk = "C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64"
    $env:Path += ';' + $sdk
    Invoke-NativeCommand -FilePath 'makeAppx.exe' -ArgumentList @('pack', '/d', "$innoDir\make_appx", '/p', "$innoDir\zzz_explorer_command_injector.appx", '/nv') -Description "Pack explorer command injector AppX"
}

function SignZZZAndItsFriends {
    if (-not $env:CI) {
        return
    }

    $files = "$innoDir\ZZZ.exe,$innoDir\cli.exe,$innoDir\zzz_explorer_command_injector.dll,$innoDir\zzz_explorer_command_injector.appx"
    & "$innoDir\sign.ps1" $files
}

function DownloadAMDGpuServices {
    # If you update the AGS SDK version, please also update the version in `crates/gpui/src/platform/windows/directx_renderer.rs`
    $url = "https://codeload.github.com/GPUOpen-LibrariesAndSDKs/AGS_SDK/zip/refs/tags/v6.3.0"
    $zipPath = ".\AGS_SDK_v6.3.0.zip"
    # Download the AGS SDK zip file
    Invoke-WebRequest -Uri $url -OutFile $zipPath
    # Extract the AGS SDK zip file
    Expand-Archive -Path $zipPath -DestinationPath "." -Force
}

function DownloadConpty {
    $url = "https://github.com/microsoft/terminal/releases/download/v1.23.13503.0/Microsoft.Windows.Console.ConPTY.1.23.251216003.nupkg"
    $zipPath = ".\Microsoft.Windows.Console.ConPTY.1.23.251216003.nupkg"
    Invoke-WebRequest -Uri $url -OutFile $zipPath
    Expand-Archive -Path $zipPath -DestinationPath ".\conpty" -Force
}

function CollectFiles {
    Move-Item -Path "$innoDir\zzz_explorer_command_injector.appx" -Destination "$innoDir\appx\zzz_explorer_command_injector.appx" -Force
    Move-Item -Path "$innoDir\zzz_explorer_command_injector.dll" -Destination "$innoDir\appx\zzz_explorer_command_injector.dll" -Force
    Move-Item -Path "$innoDir\cli.exe" -Destination "$innoDir\bin\zzz.exe" -Force
    Move-Item -Path "$innoDir\zzz.sh" -Destination "$innoDir\bin\zzz" -Force
    Move-Item -Path "$innoDir\zzz.cmd" -Destination "$innoDir\bin\zzz.cmd" -Force
    if($Architecture -eq "aarch64") {
        New-Item -Type Directory -Path "$innoDir\arm64" -Force
        Move-Item -Path ".\conpty\build\native\runtimes\arm64\OpenConsole.exe" -Destination "$innoDir\arm64\OpenConsole.exe" -Force
        Move-Item -Path ".\conpty\runtimes\win-arm64\native\conpty.dll" -Destination "$innoDir\conpty.dll" -Force
    }
    else {
        New-Item -Type Directory -Path "$innoDir\x64" -Force
        New-Item -Type Directory -Path "$innoDir\arm64" -Force
        Move-Item -Path ".\AGS_SDK-6.3.0\ags_lib\lib\amd_ags_x64.dll" -Destination "$innoDir\amd_ags_x64.dll" -Force
        Move-Item -Path ".\conpty\build\native\runtimes\x64\OpenConsole.exe" -Destination "$innoDir\x64\OpenConsole.exe" -Force
        Move-Item -Path ".\conpty\build\native\runtimes\arm64\OpenConsole.exe" -Destination "$innoDir\arm64\OpenConsole.exe" -Force
        Move-Item -Path ".\conpty\runtimes\win-x64\native\conpty.dll" -Destination "$innoDir\conpty.dll" -Force
    }
}

function BuildInstaller {
    $issFilePath = "$innoDir\zzz.iss"
    switch ($channel) {
        "stable" {
            $appId = "{{2DB0DA96-CA55-49BB-AF4F-64AF36A86712}"
            $appIconName = "app-icon"
            $appName = "ZZZ"
            $appDisplayName = "ZZZ"
            $appSetupName = "ZZZ-$Architecture"
            # The mutex name here should match the mutex name in crates\zzz\src\zzz\windows_only_instance.rs
            $appMutex = "ZZZ-Stable-Instance-Mutex"
            $appExeName = "ZZZ"
            $regValueName = "ZZZ"
            $appUserId = "ZZZIndustries.ZZZ"
            $appShellNameShort = "Z&ZZ"
            $appAppxFullName = "ZZZIndustries.ZZZ_1.0.0.0_neutral__japxn1gcva8rg"
        }
        "preview" {
            $appId = "{{F70E4811-D0E2-4D88-AC99-D63752799F95}"
            $appIconName = "app-icon-preview"
            $appName = "ZZZ Preview"
            $appDisplayName = "ZZZ Preview"
            $appSetupName = "ZZZ-$Architecture"
            # The mutex name here should match the mutex name in crates\zzz\src\zzz\windows_only_instance.rs
            $appMutex = "ZZZ-Preview-Instance-Mutex"
            $appExeName = "ZZZ"
            $regValueName = "ZZZPreview"
            $appUserId = "ZZZIndustries.ZZZ.Preview"
            $appShellNameShort = "Z&ZZ Preview"
            $appAppxFullName = "ZZZIndustries.ZZZ.Preview_1.0.0.0_neutral__japxn1gcva8rg"
        }
        "nightly" {
            $appId = "{{1BDB21D3-14E7-433C-843C-9C97382B2FE0}"
            $appIconName = "app-icon-nightly"
            $appName = "ZZZ Nightly"
            $appDisplayName = "ZZZ Nightly"
            $appSetupName = "ZZZ-$Architecture"
            # The mutex name here should match the mutex name in crates\zzz\src\zzz\windows_only_instance.rs
            $appMutex = "ZZZ-Nightly-Instance-Mutex"
            $appExeName = "ZZZ"
            $regValueName = "ZZZNightly"
            $appUserId = "ZZZIndustries.ZZZ.Nightly"
            $appShellNameShort = "Z&ZZ Editor Nightly"
            $appAppxFullName = "ZZZIndustries.ZZZ.Nightly_1.0.0.0_neutral__japxn1gcva8rg"
        }
        "dev" {
            $appId = "{{8357632E-24A4-4F32-BA97-E575B4D1FE5D}"
            $appIconName = "app-icon-dev"
            $appName = "ZZZ Dev"
            $appDisplayName = "ZZZ Dev"
            $appSetupName = "ZZZ-$Architecture"
            # The mutex name here should match the mutex name in crates\zzz\src\zzz\windows_only_instance.rs
            $appMutex = "ZZZ-Dev-Instance-Mutex"
            $appExeName = "ZZZ"
            $regValueName = "ZZZDev"
            $appUserId = "ZZZIndustries.ZZZ.Dev"
            $appShellNameShort = "Z&ZZ Dev"
            $appAppxFullName = "ZZZIndustries.ZZZ.Dev_1.0.0.0_neutral__japxn1gcva8rg"
        }
        default {
            Write-Error "can't bundle installer for $channel."
            exit 1
        }
    }

    # Windows runner 2022 default has iscc in PATH, https://github.com/actions/runner-images/blob/main/images/windows/Windows2022-Readme.md
    # Currently, we are using Windows 2022 runner.
    # Windows runner 2025 doesn't have iscc in PATH for now, https://github.com/actions/runner-images/issues/11228
    $innoSetupPath = "C:\Program Files (x86)\Inno Setup 6\ISCC.exe"

    $definitions = @{
        "AppId"          = $appId
        "AppIconName"    = $appIconName
        "OutputDir"      = "$env:ZZZ_WORKSPACE\target"
        "AppSetupName"   = $appSetupName
        "AppName"        = $appName
        "AppDisplayName" = $appDisplayName
        "RegValueName"   = $regValueName
        "AppMutex"       = $appMutex
        "AppExeName"     = $appExeName
        "ResourcesDir"   = "$innoDir"
        "ShellNameShort" = $appShellNameShort
        "AppUserId"      = $appUserId
        "Version"        = "$env:RELEASE_VERSION"
        "SourceDir"      = "$env:ZZZ_WORKSPACE"
        "AppxFullName"   = $appAppxFullName
    }

    $defs = @()
    foreach ($key in $definitions.Keys) {
        $defs += "/d$key=`"$($definitions[$key])`""
    }

    $innoArgs = @($issFilePath) + $defs
    if($env:CI) {
        $signTool = "powershell.exe -ExecutionPolicy Bypass -File $innoDir\sign.ps1 `$f"
        $innoArgs += "/sDefaultsign=`"$signTool`""
    }

    # Execute Inno Setup
    Write-Host "🚀 Running Inno Setup: $innoSetupPath $innoArgs"
    $process = Start-Process -FilePath $innoSetupPath -ArgumentList $innoArgs -NoNewWindow -Wait -PassThru

    if ($process.ExitCode -eq 0) {
        Write-Host "✅ Inno Setup successfully compiled the installer"
        Write-Output "SETUP_PATH=target/$appSetupName.exe" >> $env:GITHUB_ENV
        $script:buildSuccess = $true
    }
    else {
        Write-Host "❌ Inno Setup failed: $($process.ExitCode)"
        $script:buildSuccess = $false
    }
}

ParseZZZWorkspace
$innoDir = "$env:ZZZ_WORKSPACE\inno\$Architecture"
$debugArchive = "$CargoOutDir\zzz-$env:RELEASE_VERSION-$env:ZZZ_RELEASE_CHANNEL.dbg.zip"
$debugStoreKey = "$env:ZZZ_RELEASE_CHANNEL/zzz-$env:RELEASE_VERSION-$env:ZZZ_RELEASE_CHANNEL.dbg.zip"

CheckEnvironmentVariables
PrepareForBundle
GenerateLicenses
BuildRemoteServer
BuildZZZAndItsFriends
MakeAppx
SignZZZAndItsFriends
ZipZZZAndItsFriendsDebug
DownloadAMDGpuServices
DownloadConpty
CollectFiles
BuildInstaller

if($env:CI) {
    UploadToSentry
}

if ($buildSuccess) {
    Write-Output "Build successful"
    if ($Install) {
        Write-Output "Installing ZZZ..."
        Start-Process -FilePath "$env:ZZZ_WORKSPACE/target/ZZZ-x64.exe"
    }
    exit 0
}
else {
    Write-Output "Build failed"
    exit 1
}
