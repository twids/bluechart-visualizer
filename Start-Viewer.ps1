param([int]$Port = 8765, [string]$ChartPath = (Join-Path $PSScriptRoot 'charts/chart.img'))
$ErrorActionPreference = 'Stop'
$viewerPython = (Get-Command python -ErrorAction Stop).Source
Write-Host "Open http://127.0.0.1:$Port in your browser. Press Ctrl+C to stop."
& $viewerPython (Join-Path $PSScriptRoot 'server.py') --chart $ChartPath --port $Port
