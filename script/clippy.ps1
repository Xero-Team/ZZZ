$ErrorActionPreference = "Stop"

Write-Host "Your PATH entries:"
$env:Path -split ";" | ForEach-Object { Write-Host "  $_" }

$needAddWorkspace = $false
if ($args -notcontains "-p" -and $args -notcontains "--package")
{
    $needAddWorkspace = $true
}

# https://stackoverflow.com/questions/41324882/how-to-run-a-powershell-script-with-verbose-output/70020655#70020655
# Set-PSDebug -Trace 2

if ($env:CARGO)
{
    $Cargo = $env:CARGO
} elseif (Get-Command "cargo" -ErrorAction SilentlyContinue)
{
    $Cargo = "cargo"
} else
{
    Write-Error "Could not find cargo in path." -ErrorAction Stop
}

if ($needAddWorkspace)
{
    $clippyScopeArgs = @($args + "--workspace")
}
else
{
    $clippyScopeArgs = @($args)
}

function Invoke-Clippy
{
    param(
        [string[]]$ScopeArgs,
        [string[]]$LintArgs,
        [switch]$AllTargets
    )

    $clippyArgs = @("clippy")
    $clippyArgs += $ScopeArgs
    $clippyArgs += "--release"
    if ($AllTargets)
    {
        $clippyArgs += "--all-targets"
    }
    $clippyArgs += "--all-features"
    $clippyArgs += "--"
    $clippyArgs += "--deny"
    $clippyArgs += "warnings"
    $clippyArgs += $LintArgs

    & $Cargo @clippyArgs
    if ($LASTEXITCODE -ne 0)
    {
        exit $LASTEXITCODE
    }
}

Invoke-Clippy -ScopeArgs $clippyScopeArgs -AllTargets -LintArgs @(
    "--deny", "clippy::manual_range_contains",
    "--deny", "clippy::match_result_ok"
)

# Run a second pass for low-noise strict manifest checks across the requested packages.
Invoke-Clippy -ScopeArgs $clippyScopeArgs -LintArgs @(
    "--warn", "clippy::negative_feature_names",
    "--warn", "clippy::wildcard_dependencies"
)

# `future_not_send` is high-signal for background or thread-safe utility crates, but
# much noisier in GPUI crates that intentionally await on main-thread local state.
if (-not $needAddWorkspace)
{
    Invoke-Clippy -ScopeArgs @($args) -LintArgs @(
        "--warn", "clippy::future_not_send"
    )
}
else
{
    Invoke-Clippy -ScopeArgs @(
        "-p", "cli",
        "-p", "compliance",
        "-p", "git",
        "-p", "language_model",
        "-p", "lsp",
        "-p", "sqlez",
        "-p", "theme",
        "-p", "watch",
        "-p", "xtask"
    ) -LintArgs @(
        "--warn", "clippy::future_not_send"
    )
}

# Run style-focused checks as a separate pass so failures stay grouped and readable.
Invoke-Clippy -ScopeArgs $clippyScopeArgs -LintArgs @(
    "--warn", "clippy::redundant_else",
    "--warn", "clippy::needless_continue",
    "--warn", "clippy::str_to_string"
)
