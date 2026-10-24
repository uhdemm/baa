# Build environment for Baa on Adam's PC (VS BuildTools 18, Vulkan SDK, bun, rust).
# Usage:  . .\scripts\win-env.ps1 ; bun run tauri dev   (or: bun run tauri build)
#
# Windows build lessons:
#  - VS18's MSBuild generator dies (MSB3491) -> force Ninja.
#  - CMake+Ninja's compiler check breaks through transcribe-cpp's NTFS junction, so
#    %LOCALAPPDATA%\tcs is kept as a FILE (junction creation fails, build uses OUT_DIR).
#  - A short CARGO_TARGET_DIR keeps the Vulkan shader build under MAX_PATH instead.
#  - Don't pre-run vcvars: the cc crate sets up MSVC itself.

$vs = "C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake"
$env:PATH = "$vs\Ninja;$vs\CMake\bin;$env:USERPROFILE\.cargo\bin;$env:USERPROFILE\.bun\bin;" + $env:PATH
$env:CMAKE_GENERATOR = "Ninja"
$env:VULKAN_SDK = [Environment]::GetEnvironmentVariable("VULKAN_SDK", "Machine")
if ($env:VULKAN_SDK) { $env:PATH = "$env:VULKAN_SDK\Bin;" + $env:PATH }
$env:CARGO_TARGET_DIR = "D:\bt"

$tcs = Join-Path $env:LOCALAPPDATA "tcs"
if (-not (Test-Path $tcs)) { Set-Content -Path $tcs -Value "file on purpose: blocks transcribe-cpp junction" }

Write-Host "Baa env ready. VULKAN_SDK=$env:VULKAN_SDK  target=$env:CARGO_TARGET_DIR"
