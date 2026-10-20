<#
.SYNOPSIS
Creates and mounts a small NTFS VHDX volume for running SortDeck's real cross-volume tests.
.NOTES
Requires Administrator privileges for diskpart VHDX mount. Run tests with:
  $env:SORTDECK_SECOND_VOLUME_DIR = "Z:\SortDeckTestVol"
  cargo test --manifest-path src-tauri\Cargo.toml --locked -- --ignored
#>
param(
  [string]$VhdxPath = "$env:TEMP\sortdeck-test-vol.vhdx",
  [string]$DriveLetter = "Z",
  [int]$SizeMB = 64
)

$ErrorActionPreference = "Stop"

$script = @"
create vdisk file="$VhdxPath" maximum=$SizeMB type=expandable
select vdisk file="$VhdxPath"
attach vdisk
create partition primary
format fs=ntfs quick label="SortDeckTest"
assign letter=$DriveLetter
"@

$scriptFile = Join-Path $env:TEMP "sortdeck-diskpart-create.txt"
Set-Content -Path $scriptFile -Value $script -Encoding ASCII
diskpart /s $scriptFile
Remove-Item $scriptFile -Force

$testDir = "${DriveLetter}:\SortDeckTestVol"
New-Item -ItemType Directory -Path $testDir -Force | Out-Null
Set-Content -Path (Join-Path $testDir ".sortdeck-test-sandbox") -Value "sortdeck-test-sandbox" -Encoding ASCII

Write-Output "Mounted test VHDX at $testDir"
