# Build and package zaluu locally on Windows (Portable .exe + Inno Setup .exe)
# without mutating tracked packaging templates (which keep the 0.0.0 sentinel).
$ErrorActionPreference = "Stop"

$RootDir = Split-Path -Parent $PSScriptRoot
Push-Location $RootDir
try {
    $MetaJson = cargo metadata --locked --no-deps --format-version 1 | ConvertFrom-Json
    $Pkg = $MetaJson.packages | Where-Object { $_.name -eq "zaluu" } | Select-Object -First 1
    $Ver = $Pkg.version
    if (-not $Ver) {
        throw "Could not resolve zaluu version from Cargo.toml"
    }

    Write-Host "=== Building zaluu v$Ver ==="
    cargo build --release --locked --bin zaluu

    New-Item -ItemType Directory -Force -Path dist | Out-Null
    $PortableOut = "dist\zaluu-$Ver-portable-windows-x86_64.exe"
    Copy-Item "target\release\zaluu.exe" $PortableOut -Force
    Write-Host "=== Built $PortableOut ==="

    if (Get-Command iscc -ErrorAction SilentlyContinue) {
        iscc "/DMyAppVersion=$Ver" "packaging\windows\installer.iss"
        $SetupOut = "dist\zaluu-$Ver-setup-windows-x86_64.exe"
        Move-Item "dist\zaluu-setup-windows-x86_64.exe" $SetupOut -Force
        Write-Host "=== Built $SetupOut ==="
    } else {
        Write-Warning "Inno Setup (iscc) not found in PATH; skipped setup installer generation."
    }
} finally {
    Pop-Location
}
