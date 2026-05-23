
function ParseZedWorkspace {
    $standardOutputPath = [System.IO.Path]::GetTempFileName()
    $standardErrorPath = [System.IO.Path]::GetTempFileName()

    try {
        $process = Start-Process -FilePath 'cargo' -ArgumentList @('metadata', '--format-version=1', '--no-deps', '--offline') -NoNewWindow -Wait -PassThru -RedirectStandardOutput $standardOutputPath -RedirectStandardError $standardErrorPath
        $standardError = Get-Content $standardErrorPath -ErrorAction SilentlyContinue

        if ($standardError) {
            Write-Output $standardError
        }

        if ($process.ExitCode -ne 0) {
            throw "cargo metadata failed with exit code $($process.ExitCode)"
        }

        $metadataJson = Get-Content $standardOutputPath -Raw
    }
    finally {
        Remove-Item $standardOutputPath -ErrorAction SilentlyContinue
        Remove-Item $standardErrorPath -ErrorAction SilentlyContinue
    }

    $metadata = $metadataJson | ConvertFrom-Json
    $env:ZED_WORKSPACE = $metadata.workspace_root
    $env:RELEASE_VERSION = $metadata.packages | Where-Object { $_.name -eq "zed" } | Select-Object -ExpandProperty version
}
