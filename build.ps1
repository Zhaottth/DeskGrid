$ErrorActionPreference = 'Stop'
$framework = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319'
if (!(Test-Path -LiteralPath (Join-Path $framework 'csc.exe'))) {
    $framework = Join-Path $env:WINDIR 'Microsoft.NET\Framework\v4.0.30319'
}
$compiler = Join-Path $framework 'csc.exe'
if (!(Test-Path -LiteralPath $compiler)) { throw 'The Windows .NET Framework C# compiler is required.' }

$sourceFolder = Join-Path $PSScriptRoot 'src'
$outputFolder = Join-Path $PSScriptRoot 'dist'
New-Item -ItemType Directory -Path $outputFolder -Force | Out-Null
$sources = @(Get-ChildItem -LiteralPath $sourceFolder -Filter '*.cs' -File | ForEach-Object FullName)
$output = Join-Path $outputFolder 'DeskGrid.exe'
$icon = Join-Path $PSScriptRoot 'assets\DeskGrid.ico'
$manifest = Join-Path $sourceFolder 'DeskGrid.manifest'
& $compiler /nologo /target:winexe /langversion:5 /optimize+ /codepage:65001 "/out:$output" "/win32icon:$icon" "/win32manifest:$manifest" /r:System.dll /r:System.Core.dll /r:System.Drawing.dll /r:System.Windows.Forms.dll /r:System.Xml.dll @sources
if ($LASTEXITCODE -ne 0) { throw "Compilation failed ($LASTEXITCODE)." }
Write-Output $output
