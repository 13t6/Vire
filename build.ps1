# Builds Vire.exe (Rust, GNU toolchain), runs the tests and assembles the release package.
# Needs: rustup stable-x86_64-pc-windows-gnu in %USERPROFILE%\.cargo, windres from C:\msys64\ucrt64\bin.
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot
$env:PATH = "$env:USERPROFILE\.cargo\bin;C:\msys64\ucrt64\bin;$env:PATH"
# Short target dir keeps build-script paths under Windows' 260-char limit.
$env:CARGO_TARGET_DIR = "$env:USERPROFILE\.cargo\viretarget"
# Panic messages embed source paths. Rewrite the profile folder and the project folder so the
# published exe never contains the builder's user name or folders.
# (CARGO_ENCODED_RUSTFLAGS: separated by 0x1F, so folder names with spaces are fine.)
$env:CARGO_ENCODED_RUSTFLAGS = "--remap-path-prefix=$env:USERPROFILE=~" + [char]0x1f + "--remap-path-prefix=$PSScriptRoot=vire"

Get-Process Vire -ErrorAction SilentlyContinue | Stop-Process -Force   # the exe can't be replaced while it runs

cargo test --release
if ($LASTEXITCODE -ne 0) { Write-Error "Tests failed - not releasing."; exit 1 }
cargo build --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
if (-not (Test-Path samples\cyber_sunset_grid.mp4)) { & .\tools\gen_samples.ps1 }

$dist = "dist\Vire"
New-Item -ItemType Directory -Force "$dist\samples" | Out-Null
Copy-Item "$env:CARGO_TARGET_DIR\release\Vire.exe" $dist -Force
Copy-Item samples\*.mp4 "$dist\samples" -Force

# Release package: the app only. Your own wallpapers and music live in Documents\Vire (and the
# settings in %APPDATA%\Vire), never inside the app folder, so nothing personal can end up in
# what you publish. This step refuses to zip anything but the files listed here.
Copy-Item tools\Vire-debug.bat $dist -Force
Copy-Item tools\README.txt $dist -Force
$allowed = @("Vire.exe", "Vire-debug.bat", "README.txt") + (Get-ChildItem samples\*.mp4 | ForEach-Object { $_.Name })
# `Data` (your wallpapers, music, settings when running from this folder) is never part of a release.
if ($stray) { Write-Error "Not releasing: unexpected files in dist: $($stray.FullName -join ', ')"; exit 1 }

# Privacy check on the binary itself: no user name, no home folder.
$text = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes("$dist\Vire.exe"))
foreach ($leak in @($env:USERNAME, $env:USERPROFILE, ($env:USERNAME -replace ' ', ''))) {
    if ($leak -and $text.Contains($leak)) { Write-Error "Not releasing: Vire.exe contains '$leak'"; exit 1 }
}

$ver = (Select-String -Path Cargo.toml -Pattern '^version = "([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
New-Item -ItemType Directory -Force release | Out-Null
Get-ChildItem release -Filter *.zip | Remove-Item -Force
$zip = "release\Vire-$ver-win64.zip"
Compress-Archive -Path (Get-ChildItem $dist | Where-Object { $_.Name -ne "Data" } | ForEach-Object { $_.FullName }) -DestinationPath $zip
Get-ChildItem $dist -Recurse -File | Where-Object { $_.FullName -notlike "*\Data\*" } | Select-Object Name, @{n='KB';e={[int]($_.Length/1KB)}}
"Release: $zip ($([int]((Get-Item $zip).Length/1KB)) KB) - Vire.exe + samples only, version $ver"
