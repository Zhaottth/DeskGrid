$ErrorActionPreference = 'Stop'
Push-Location -LiteralPath $PSScriptRoot
try {
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw "Cargo build failed ($LASTEXITCODE)." }

    $metadata = cargo metadata --locked --no-deps --format-version 1
    if ($LASTEXITCODE -ne 0) { throw "Cargo metadata failed ($LASTEXITCODE)." }
    $targetFolder = ($metadata | ConvertFrom-Json).target_directory
    $binary = Join-Path $targetFolder 'release\deskgrid.exe'
    $output = Join-Path $PSScriptRoot 'dist\DeskGrid.exe'
    New-Item -ItemType Directory -Path (Split-Path -Parent $output) -Force | Out-Null
    Copy-Item -LiteralPath $binary -Destination $output -Force
    Write-Output $output
}
finally {
    Pop-Location
}
