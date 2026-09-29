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

# The workspace lint table is the single source of truth for the deny set, and
# this pass runs with --all-targets so tests, benches, and examples are held to
# the same bar as library and binary targets.
Invoke-Clippy -ScopeArgs $clippyScopeArgs -AllTargets

# Run a second pass for low-noise strict manifest checks. These stay on the
# command line rather than in [workspace.lints] because they are manifest lints
# that path-dependency crates without [lints] workspace = true would miss.
# Manifest lints are target-independent, so this pass does not use --all-targets.
Invoke-Clippy -ScopeArgs $clippyScopeArgs -LintArgs @(
    "--deny", "clippy::negative_feature_names",
    "--deny", "clippy::wildcard_dependencies"
)

# `future_not_send` is high-signal for background or thread-safe utility crates,
# but much noisier in GPUI crates that intentionally await on main-thread local
# state, and in test futures. Restrict it to library/binary targets: enforce it
# for the curated utility packages, and keep it advisory for an explicit `-p`
# selection so a targeted run on a GPUI crate is not blocked.
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
        "--deny", "clippy::future_not_send"
    )
}
