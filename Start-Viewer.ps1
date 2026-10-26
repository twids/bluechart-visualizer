param([int]$Port = 8765)
$ErrorActionPreference = 'Stop'
$bundledPython = Join-Path $env:USERPROFILE '.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe'
if (Test-Path -LiteralPath $bundledPython) {
    $viewerPython = $bundledPython
} else {
    $viewerPython = (Get-Command python -ErrorAction Stop).Source
}
Write-Host "Öppna http://127.0.0.1:$Port i webbläsaren. Avsluta med Ctrl+C."
& $viewerPython (Join-Path $PSScriptRoot 'server.py') --port $Port
