$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Resolve-Path (Join-Path $scriptDir "..\..")
$distDir = Join-Path $repoRoot "dist"
$stageDir = Join-Path $repoRoot "target\packaging\windows\stage"

$cargoToml = Join-Path $repoRoot "alterm\Cargo.toml"
$versionLine = Select-String -Path $cargoToml -Pattern '^version = "(.+)"$' | Select-Object -First 1
if (-not $versionLine) {
    throw "Could not determine package version from $cargoToml"
}
$version = $versionLine.Matches[0].Groups[1].Value

$target = if ($env:CARGO_BUILD_TARGET) { $env:CARGO_BUILD_TARGET } else { $env:PROCESSOR_ARCHITECTURE }
$arch = switch -Regex ($target) {
    'aarch64|arm64' { 'arm64'; break }
    'x86_64|amd64' { 'x64'; break }
    default { 'x64' }
}

New-Item -ItemType Directory -Force -Path $distDir | Out-Null
New-Item -ItemType Directory -Force -Path $stageDir | Out-Null

Push-Location $repoRoot
try {
    cargo build --release --package alterm
} finally {
    Pop-Location
}

Copy-Item (Join-Path $repoRoot "target\release\alterm.exe") (Join-Path $stageDir "alterm.exe") -Force
Copy-Item (Join-Path $repoRoot "README.md") (Join-Path $stageDir "README.txt") -Force
Copy-Item (Join-Path $repoRoot "config\default.toml") (Join-Path $stageDir "config.toml.example") -Force
Copy-Item (Join-Path $repoRoot "config\hooks.lua.example") (Join-Path $stageDir "hooks.lua.example") -Force

$iscc = Get-Command ISCC.exe -ErrorAction SilentlyContinue
if (-not $iscc) {
    throw "Inno Setup compiler (ISCC.exe) was not found in PATH."
}

& $iscc.Source `
    "/DMyAppVersion=$version" `
    "/DMySourceDir=$stageDir" `
    "/DMyOutputDir=$distDir" `
    "/DMyOutputArch=$arch" `
    (Join-Path $scriptDir "alterm.iss")

Write-Host "Installer created in $distDir"
