# Helpers for resolving the newest GitHub release asset, so packaging scripts
# use current tool releases instead of hand-pinned versions and checksums.
#
# Set GITHUB_TOKEN (or GH_TOKEN) to raise the GitHub API rate limit.

function Get-LatestReleaseAsset {
    param(
        [Parameter(Mandatory)][string]$Repository,
        [Parameter(Mandatory)][string]$Pattern,
        [string]$Tag
    )

    $headers = @{
        'User-Agent' = 'ZZZ-Setup'
        'Accept'     = 'application/vnd.github+json'
    }
    $token = if ($env:GITHUB_TOKEN) { $env:GITHUB_TOKEN } else { $env:GH_TOKEN }
    if ($token) {
        $headers['Authorization'] = "Bearer $token"
    }

    $path = if ($Tag) { "releases/tags/$Tag" } else { 'releases/latest' }
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repository/$path" -Headers $headers

    $asset = $release.assets | Where-Object { $_.name -like $Pattern } | Select-Object -First 1
    if (-not $asset) {
        throw "No $Repository release asset matches '$Pattern'."
    }
    $digest = $asset.digest -replace '^sha256:', ''
    if (-not $digest) {
        throw "$($asset.name) publishes no checksum; refusing an unverified download."
    }

    [pscustomobject]@{
        Tag    = $release.tag_name
        Name   = $asset.name
        Sha256 = $digest
        Url    = $asset.browser_download_url
    }
}

function Save-VerifiedDownload {
    param(
        [Parameter(Mandatory)][string]$Url,
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$Sha256,
        [string]$Label = $Path
    )

    $partial = "$Path.part"
    Remove-Item -Path $partial -Force -ErrorAction SilentlyContinue
    Invoke-WebRequest -Uri $Url -OutFile $partial
    $actual = (Get-FileHash -Path $partial -Algorithm SHA256).Hash
    if ($actual -ne $Sha256.ToUpperInvariant()) {
        Remove-Item -Path $partial -Force -ErrorAction SilentlyContinue
        throw "Checksum mismatch for ${Label}: expected $Sha256, got $actual"
    }
    Move-Item -Path $partial -Destination $Path -Force
}
