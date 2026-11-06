<#
.SYNOPSIS
  Packages the Actrium Windows client into distributable artifacts.

.DESCRIPTION
  Runs on a Windows host (PowerShell 5.1 or later) and produces, under
  dist/windows/:

    1. A reproducible self-contained publish folder
       (dotnet publish -c Release -r win-x64 --self-contained true) with
       actrium_core.dll flattened at the application root.
    2. A portable zip: Actrium-<version>-win-x64.zip.
    3. A per-user installer: Actrium-<version>-win-x64-setup.exe, built with
       Inno Setup 6 (ISCC.exe) from windows/packaging/
       actrium-installer.iss. When Inno Setup is not installed the script
       still succeeds and produces the portable zip only.

  Version single-sourcing: windows/VERSION is the only version
  authority. The csproj reads it into <Version> (assembly/file/product
  version) and this script reads it for artifact names and the installer
  AppVersion, so the binary stamp and the installer stamp cannot drift.

  Installer policy (see actrium-installer.iss for the implementation):
    - Installs per-user under %LOCALAPPDATA%\Programs\Actrium; never requests
      for administrator rights.
    - Creates a per-user Start Menu shortcut.
    - Detects the WebView2 Evergreen runtime via the EdgeUpdate Clients
      registry keys and runs the official Microsoft bootstrapper
      (downloaded by this script at package time) only when it is missing.
    - Uninstall removes the installed binaries and shortcuts but leaves
      user profile data (%LOCALAPPDATA%\Actrium\Actrium\Windows and the
      WebView2 browsing profile) in place, so a reinstall restores the
      user's session, bookmarks and history.

  KNOWN GAP - code signing: the produced executables, zip payload and
  installer are NOT signed because no code-signing certificate exists yet.
  SmartScreen will warn on first run/install. When a certificate becomes
  available, sign Actrium.exe and the setup exe with signtool (present in the
  Windows SDK) as a post-step here.

.PARAMETER Configuration
  Build configuration, default Release.

.PARAMETER Runtime
  .NET runtime identifier, default win-x64.

.PARAMETER DotnetExe
  Full path to dotnet.exe. Defaults to %DOTNET_ROOT%\dotnet.exe, then to
  dotnet on PATH.

.PARAMETER SkipInstaller
  Produce the portable zip only, even if Inno Setup is installed.

.PARAMETER SkipWebView2Bootstrapper
  Do not download the WebView2 Evergreen bootstrapper. The installer is
  then built without the embedded bootstrapper and only warns when the
  runtime is missing (offline packaging escape hatch).
#>
[CmdletBinding()]
param(
    [string]$Configuration = 'Release',
    [string]$Runtime = 'win-x64',
    [string]$DotnetExe = '',
    [switch]$SkipInstaller,
    [switch]$SkipWebView2Bootstrapper
)

$ErrorActionPreference = 'Stop'

function Write-Step([string]$Message) { Write-Host "[step] $Message" }
function Write-Ok([string]$Message) { Write-Host "[ok] $Message" }
function Write-Note([string]$Message) { Write-Host "[note] $Message" }

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)

# --- Version: single source of truth is windows/VERSION. ---------------
$versionFile = Join-Path $repoRoot 'windows\VERSION'
if (-not (Test-Path -LiteralPath $versionFile)) {
    throw "version file not found: $versionFile"
}
$version = (Get-Content -LiteralPath $versionFile -Raw).Trim()
if ($version -notmatch '^\d+\.\d+\.\d+$') {
    throw "windows/VERSION must contain a MAJOR.MINOR.PATCH version, got '$version'"
}
Write-Ok "packaging version $version (from windows/VERSION)"

# --- Locate dotnet. ----------------------------------------------------------
if (-not $DotnetExe) {
    if ($env:DOTNET_ROOT) {
        $candidate = Join-Path $env:DOTNET_ROOT 'dotnet.exe'
        if (Test-Path -LiteralPath $candidate) { $DotnetExe = $candidate }
    }
}
if (-not $DotnetExe) {
    $command = Get-Command dotnet -ErrorAction SilentlyContinue
    if ($command) { $DotnetExe = $command.Source }
}
if (-not $DotnetExe) {
    throw 'dotnet.exe not found: pass -DotnetExe, set DOTNET_ROOT, or put dotnet on PATH'
}
Write-Ok "using dotnet at $DotnetExe"

# --- Publish (self-contained, single output folder). -------------------------
$distRoot = Join-Path $repoRoot 'dist\windows'
$publishDir = Join-Path $distRoot "publish\Actrium-$version-$Runtime"
$project = Join-Path $repoRoot 'windows\Actrium.Windows.csproj'

if (Test-Path -LiteralPath $publishDir) {
    Write-Step "removing stale publish folder $publishDir"
    Remove-Item -LiteralPath $publishDir -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $publishDir | Out-Null

Write-Step "dotnet publish -c $Configuration -r $Runtime --self-contained true"
& $DotnetExe publish $project -c $Configuration -r $Runtime --self-contained true -o $publishDir
if ($LASTEXITCODE -ne 0) {
    throw "dotnet publish failed with exit code $LASTEXITCODE"
}

# Fail fast when the publish payload is incomplete: the app itself loads
# actrium_core.dll fail-fast from the application root, so a package without
# it would install a client that cannot start.
foreach ($required in @('Actrium.exe', 'actrium_core.dll', 'WebView2Loader.dll')) {
    $requiredPath = Join-Path $publishDir $required
    if (-not (Test-Path -LiteralPath $requiredPath)) {
        throw "publish output is missing $required at its root: $requiredPath"
    }
}
Write-Ok 'publish root carries Actrium.exe, actrium_core.dll and WebView2Loader.dll'

# The binary stamp must agree with windows/VERSION; a mismatch means the
# csproj no longer reads the version file and the single source has drifted.
$exeInfo = (Get-Item -LiteralPath (Join-Path $publishDir 'Actrium.exe')).VersionInfo
if (-not $exeInfo.ProductVersion.StartsWith($version)) {
    throw "Actrium.exe ProductVersion '$($exeInfo.ProductVersion)' does not match windows/VERSION '$version'"
}
Write-Ok "Actrium.exe stamped ProductVersion=$($exeInfo.ProductVersion) FileVersion=$($exeInfo.FileVersion)"

# --- Portable zip. ------------------------------------------------------------
$zipPath = Join-Path $distRoot "Actrium-$version-$Runtime.zip"
if (Test-Path -LiteralPath $zipPath) { Remove-Item -LiteralPath $zipPath -Force }
Write-Step "compressing portable zip $zipPath"
Compress-Archive -Path (Join-Path $publishDir '*') -DestinationPath $zipPath
$zipSize = (Get-Item -LiteralPath $zipPath).Length
Write-Ok ("portable zip ready: {0} ({1:N0} bytes)" -f $zipPath, $zipSize)

if ($SkipInstaller) {
    Write-Note 'installer skipped by request (-SkipInstaller); portable zip only'
    exit 0
}

# --- Installer (Inno Setup 6, optional tooling). ------------------------------
$isccCandidates = @(
    (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe'),
    (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'),
    (Join-Path $env:ProgramFiles 'Inno Setup 6\ISCC.exe')
)
$iscc = $null
foreach ($candidate in $isccCandidates) {
    if ($candidate -and (Test-Path -LiteralPath $candidate)) { $iscc = $candidate; break }
}
if (-not $iscc) {
    $command = Get-Command iscc -ErrorAction SilentlyContinue
    if ($command) { $iscc = $command.Source }
}
if (-not $iscc) {
    Write-Warning 'Inno Setup 6 (ISCC.exe) is not installed; produced the portable zip only.'
    Write-Warning 'Install Inno Setup 6 (per-user install is enough) and re-run to also build the installer.'
    exit 0
}
Write-Ok "using Inno Setup compiler at $iscc"

# WebView2 Evergreen bootstrapper (~2 MB): embedded into the installer and run
# only when the runtime is missing on the target machine. Official permalink,
# always serves the current bootstrapper.
$bootstrapperPath = Join-Path $distRoot 'MicrosoftEdgeWebView2Setup.exe'
$withBootstrapper = $false
if ($SkipWebView2Bootstrapper) {
    Write-Note 'WebView2 bootstrapper download skipped by request; installer will not embed it'
} else {
    if (-not (Test-Path -LiteralPath $bootstrapperPath)) {
        Write-Step 'downloading WebView2 Evergreen bootstrapper'
        Invoke-WebRequest -UseBasicParsing -Uri 'https://go.microsoft.com/fwlink/p/?LinkId=2124703' -OutFile $bootstrapperPath
    } else {
        Write-Note 'reusing previously downloaded WebView2 bootstrapper'
    }
    $withBootstrapper = $true
}

$issScript = Join-Path $repoRoot 'windows\packaging\actrium-installer.iss'
$setupBaseName = "Actrium-$version-$Runtime-setup"
$isccArgs = @(
    "/DAppVersion=$version",
    "/DPublishDir=$publishDir",
    "/O$distRoot",
    "/F$setupBaseName"
)
if ($withBootstrapper) {
    $isccArgs += "/DBootstrapperPath=$bootstrapperPath"
}

Write-Step "compiling installer $setupBaseName.exe"
& $iscc @isccArgs $issScript
if ($LASTEXITCODE -ne 0) {
    throw "ISCC failed with exit code $LASTEXITCODE"
}
$setupPath = Join-Path $distRoot "$setupBaseName.exe"
$setupSize = (Get-Item -LiteralPath $setupPath).Length
Write-Ok ("installer ready: {0} ({1:N0} bytes)" -f $setupPath, $setupSize)
Write-Note 'artifacts are unsigned (no code-signing certificate yet); SmartScreen will warn'
