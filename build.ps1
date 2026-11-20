param([switch]$Package)

$ErrorActionPreference = 'Stop'
Push-Location -LiteralPath $PSScriptRoot
try {
    $cargoArgs = @('build', '--release', '--locked')
    if ($Package) {
        $windowsTarget = 'x86_64-pc-windows-msvc'
        $cargoArgs += @('--target', $windowsTarget)
    }
    cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) { throw "Cargo build failed ($LASTEXITCODE)." }

    $metadata = cargo metadata --locked --no-deps --format-version 1
    if ($LASTEXITCODE -ne 0) { throw "Cargo metadata failed ($LASTEXITCODE)." }
    $project = $metadata | ConvertFrom-Json
    $targetFolder = $project.target_directory
    $dist = Join-Path $PSScriptRoot 'dist'
    New-Item -ItemType Directory -Path $dist -Force | Out-Null

    if ($Package) {
        $version = ($project.packages | Where-Object { $_.name -eq 'deskgrid' }).version
        $binary = Join-Path $targetFolder "$windowsTarget\release\deskgrid.exe"
        $output = Join-Path $dist "DeskGrid-$version-windows-x64.zip"
        $rustRoot = rustc --print sysroot
        if ($LASTEXITCODE -ne 0) { throw "rustc could not locate the active toolchain ($LASTEXITCODE)." }
        $rustDocs = Join-Path $rustRoot 'share\doc\rust'
        $rustCopyright = Join-Path $rustDocs 'COPYRIGHT-library.html'
        if (!(Test-Path -LiteralPath $rustCopyright -PathType Leaf)) {
            throw 'Rust standard library notices are required for the release ZIP. Install the active toolchain documentation with rustup component add rust-docs.'
        }
        $files = [ordered]@{
            'DeskGrid.exe'                         = $binary
            'LICENSE'                              = Join-Path $PSScriptRoot 'LICENSE'
            'THIRD_PARTY_NOTICES.txt'               = Join-Path $PSScriptRoot 'THIRD_PARTY_NOTICES.txt'
            'rust-licenses/COPYRIGHT-library.html'  = $rustCopyright
            'rust-licenses/Apache-2.0.txt'          = Join-Path $rustDocs 'licenses\Apache-2.0.txt'
            'rust-licenses/MIT.txt'                 = Join-Path $rustDocs 'licenses\MIT.txt'
        }
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $temporary = Join-Path $dist ([guid]::NewGuid().ToString('N') + '.tmp')
        try {
            $zip = [IO.Compression.ZipFile]::Open($temporary, [IO.Compression.ZipArchiveMode]::Create)
            try {
                foreach ($entry in $files.GetEnumerator()) {
                    [IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
                        $zip, $entry.Value, $entry.Key, [IO.Compression.CompressionLevel]::Optimal
                    ) | Out-Null
                }
            }
            finally {
                $zip.Dispose()
            }
            Move-Item -LiteralPath $temporary -Destination $output -Force
        }
        finally {
            if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary }
        }
    }
    else {
        $binary = Join-Path $targetFolder 'release\deskgrid.exe'
        $output = Join-Path $dist 'DeskGrid.exe'
        Copy-Item -LiteralPath $binary -Destination $output -Force
    }
    Write-Output $output
}
finally {
    Pop-Location
}
