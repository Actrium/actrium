#!/usr/bin/env bash
# Packages the Actro Windows client (portable zip + per-user installer).
#
# This is the repo entry point; the real pipeline lives in
# scripts/actrium/package-windows.ps1 because packaging must run on a Windows host
# (dotnet publish for win-x64 plus the optional Inno Setup 6 compile). On a
# Windows bash (Git Bash / MSYS / Cygwin) this wrapper hands straight over
# to PowerShell; on other hosts it documents the exact command to run on a
# Windows machine instead of failing cryptically.
#
# Outputs (under dist/windows/, gitignored):
#   - Actro-<version>-win-x64.zip           portable, run Actro.exe in place
#   - Actro-<version>-win-x64-setup.exe     per-user installer (needs Inno
#                                           Setup 6; skipped with a warning
#                                           when ISCC.exe is absent)
#
# Version single source: windows/VERSION (also read by the csproj).
#
# KNOWN GAP: artifacts are unsigned - no code-signing certificate exists
# yet, so SmartScreen warns on first run/install. Uninstall removes the
# installed binaries and shortcuts but deliberately leaves the user's
# profile data (%LOCALAPPDATA%\Actrium\Actro\Windows and the WebView2
# browsing profile) in place; the policy is documented in
# windows/packaging/actro-installer.iss.
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
ps_script="$repo_root/scripts/actrium/package-windows.ps1"

case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*)
    exec powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$ps_script" "$@"
    ;;
  *)
    echo "error: Windows packaging must run on a Windows host." >&2
    echo "On the Windows machine, run:" >&2
    echo "  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/actrium/package-windows.ps1" >&2
    echo "(set DOTNET_ROOT or pass -DotnetExe when dotnet is not on PATH)" >&2
    exit 1
    ;;
esac
