# grit-simple nightly installer for Windows - installs the latest manually-triggered CI build.
#
# Unlike the release installer (docs/install.ps1), nightly builds are not
# published as GitHub Releases. They live as a `gs-<target>-nightly` artifact on
# the most recent manual (workflow_dispatch) run of the Release workflow. GitHub
# requires authentication to download workflow artifacts, so this script drives
# the GitHub CLI (`gh`) - log in first with `gh auth login`.
#
# Usage: irm grit-scm.com/install-nightly.ps1 | iex
#
# Override the install location with $env:GRIT_INSTALL_DIR before running.

$ErrorActionPreference = 'Stop'

$Repo = 'gitbutlerapp/grit'
$InstallDir = if ($env:GRIT_INSTALL_DIR) { $env:GRIT_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'grit\bin' }

# Windows ARM64 runs x86_64 binaries via emulation, so x86_64 is the only target we ship.
$Target = 'x86_64-pc-windows-msvc'

if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
  throw "The GitHub CLI (gh) is required. Install it from https://cli.github.com and run 'gh auth login'."
}

# Find the most recent successful manual (workflow_dispatch) build.
Write-Host "Looking up the latest manual build..."
$RunId = gh run list --repo $Repo --workflow release.yml `
  --event workflow_dispatch --status success --limit 1 `
  --json databaseId --jq '.[0].databaseId'

if ([string]::IsNullOrWhiteSpace($RunId)) {
  throw "No successful manual build found. Trigger one from the Actions tab (Run workflow) and try again."
}
Write-Host "Using run #$RunId"

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("grit-" + [System.IO.Path]::GetRandomFileName())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
  $Artifact = "gs-$Target-nightly"
  Write-Host "Downloading artifact: $Artifact"
  gh run download $RunId --repo $Repo -n $Artifact -D $tmp

  Expand-Archive -Path (Join-Path $tmp "gs-$Target.zip") -DestinationPath $tmp -Force

  New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
  Copy-Item -Path (Join-Path $tmp 'gs.exe') -Destination (Join-Path $InstallDir 'gs.exe') -Force
} finally {
  Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}

Write-Host "Installed gs to $InstallDir\gs.exe"

# Add the install dir to the user PATH if it isn't there already.
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (($userPath -split ';') -notcontains $InstallDir) {
  $newPath = if ([string]::IsNullOrEmpty($userPath)) { $InstallDir } else { "$userPath;$InstallDir" }
  [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
  $env:Path = "$env:Path;$InstallDir"
  Write-Host "Added $InstallDir to your user PATH (restart open terminals to pick it up)."
}

& "$InstallDir\gs.exe" --version
