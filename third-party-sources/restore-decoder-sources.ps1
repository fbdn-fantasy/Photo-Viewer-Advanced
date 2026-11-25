param([string]$Destination = (Join-Path (Split-Path -Parent $PSScriptRoot) 'decoder-rebuild'))
$ErrorActionPreference = 'Stop'
$PvaDestination = [IO.Path]::GetFullPath($Destination)
if (Test-Path -LiteralPath $PvaDestination) { throw "Destination already exists; choose a new directory: $PvaDestination" }
$PvaManifest = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'manifest.json') -Raw | ConvertFrom-Json
New-Item -ItemType Directory -Path $PvaDestination | Out-Null
foreach ($source in $PvaManifest.sources) {
    if (!$source.repository.StartsWith('https://github.com/ImageMagick/')) { continue }
    $archive = Join-Path $PSScriptRoot $source.filename
    $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($hash -ne $source.sha256) { throw "Archive checksum failed: $($source.filename)" }
    $repo = ($source.repository -split '/')[-1]
    if ($repo -eq 'Windows') { $target = $PvaDestination }
    elseif ($source.role -eq 'dependency-configure') { $target = Join-Path $PvaDestination 'Dependencies\Configure' }
    elseif ($repo -in @('ImageMagick','Configure','Dependencies')) { $target = Join-Path $PvaDestination $repo }
    else { $target = Join-Path $PvaDestination ('Dependencies\Dependencies\' + $repo) }
    New-Item -ItemType Directory -Force -Path $target | Out-Null
    & tar.exe -xf $archive --strip-components=1 -C $target
    if ($LASTEXITCODE) { throw "Archive extraction failed: $($source.filename)" }
    Write-Host "Restored $repo"
}
Write-Host "Source tree restored to $PvaDestination. See Readme.md for upstream build requirements."
