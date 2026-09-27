# Install or upgrade `ta` on Windows from this repository's GitHub Releases,
# using your own `gh` login (the releases are not public).
#
#   gh api repos/calebl/text-adventure-terminal/contents/install.ps1 `
#     -H 'Accept: application/vnd.github.raw' | Out-String | Invoke-Expression
#
# Environment:
#   TA_VERSION      a release tag such as v0.1.0 (default: the latest release)
#   TA_INSTALL_DIR  where to put ta.exe (default: %LOCALAPPDATA%\Programs\ta)
#   TA_RELEASE_REPO owner/name to install from (default: calebl/text-adventure-terminal)
$ErrorActionPreference = 'Stop'

$repo = if ($env:TA_RELEASE_REPO) { $env:TA_RELEASE_REPO } else { 'calebl/text-adventure-terminal' }
$installDir = if ($env:TA_INSTALL_DIR) { $env:TA_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\ta' }
$version = $env:TA_VERSION

if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
  throw 'The GitHub CLI (gh) is required: https://cli.github.com'
}
gh auth status *> $null
if ($LASTEXITCODE -ne 0) { throw 'gh is not signed in; run: gh auth login' }

$target = 'x86_64-pc-windows-msvc'
$asset = "ta-$target.zip"
$tmp = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
  if (-not $version) {
    $version = gh release view --repo $repo --json tagName --jq .tagName
    if ($LASTEXITCODE -ne 0) { throw "Could not find a release in $repo (does your gh login have access?)" }
  }
  Write-Host "Downloading ta $version for $target"
  gh release download $version --repo $repo --dir $tmp --pattern $asset --pattern SHA256SUMS
  if ($LASTEXITCODE -ne 0) { throw "Could not download $asset from $repo $version" }

  $line = Get-Content (Join-Path $tmp 'SHA256SUMS') |
    Where-Object { ($_ -split '\s+')[1].TrimStart('*') -eq $asset } |
    Select-Object -First 1
  if (-not $line) { throw "SHA256SUMS does not list $asset" }
  $expected = ($line -split '\s+')[0].ToLower()
  $actual = (Get-FileHash -Algorithm SHA256 (Join-Path $tmp $asset)).Hash.ToLower()
  if ($actual -ne $expected) { throw "Checksum mismatch for $asset; nothing was installed" }

  Expand-Archive -Path (Join-Path $tmp $asset) -DestinationPath $tmp -Force
  New-Item -ItemType Directory -Force -Path $installDir | Out-Null
  $dest = Join-Path $installDir 'ta.exe'
  if (Test-Path $dest) {
    # A running ta.exe cannot be overwritten, but it can be renamed.
    Move-Item -Force $dest "$dest.old"
  }
  Copy-Item (Join-Path $tmp 'ta.exe') $dest
  Remove-Item -Force "$dest.old" -ErrorAction SilentlyContinue
  Write-Host "Installed ta $version to $dest"

  $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
  if (-not (($userPath -split ';') -contains $installDir)) {
    [Environment]::SetEnvironmentVariable('Path', "$userPath;$installDir", 'User')
    Write-Host "Added $installDir to your user PATH; open a new terminal to use ta."
  }
} finally {
  Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
