param([int]$Port = 8765, [string]$ChartPath = (Join-Path $PSScriptRoot 'charts/chart.img'), [string]$ChartsDir)
$ErrorActionPreference = 'Stop'
$viewerBinary = Join-Path $PSScriptRoot 'target/release/bluechart.exe'
if (!(Test-Path -LiteralPath $viewerBinary)) {
    Push-Location $PSScriptRoot
    try { cargo build --release --locked; if ($LASTEXITCODE -ne 0) { throw 'Rust build failed' } } finally { Pop-Location }
}
Write-Host "Open http://127.0.0.1:$Port in your browser. Press Ctrl+C to stop."
$viewerArguments = @('serve', '--chart', $ChartPath, '--port', $Port, '--overview', (Join-Path $PSScriptRoot 'cache/overview.sqlite3'), '--cache-dir', (Join-Path $PSScriptRoot 'cache'))
if ($ChartsDir) { $viewerArguments += @('--charts-dir', $ChartsDir) }
& $viewerBinary @viewerArguments
