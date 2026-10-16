#!/bin/sh
set -u

if [ "$#" -eq 0 ]; then
  echo "usage: $0 <xcodebuild command...>" >&2
  exit 64
fi

log_file=$(mktemp "${TMPDIR:-/tmp}/actrium-xcodebuild.XXXXXX") || exit 1

"$@" >"$log_file" 2>&1
status=$?

awk '
/DVTErrorPresenter: Unable to load simulator devices\./ {
  skipping_core_simulator = 1
  separators = 0
  next
}

skipping_core_simulator && /^--$/ {
  separators += 1
  if (separators >= 2) {
    skipping_core_simulator = 0
    separators = 0
  }
  next
}

skipping_core_simulator && /^$/ {
  next
}

skipping_core_simulator && /^Domain: DVTCoreSimulatorAdditionsErrorDomain$/ {
  next
}

skipping_core_simulator && /^Code: 3$/ {
  next
}

skipping_core_simulator && /^Failure Reason: The version of the CoreSimulator framework installed on this Mac is out-of-date/ {
  next
}

skipping_core_simulator && /^Recovery Suggestion: Please ensure that you have installed all available updates to your Mac/ {
  next
}

skipping_core_simulator && /^CoreSimulator is out of date\. Current version \([0-9.]+\) is older than build version \([0-9.]+\)\.$/ {
  next
}

skipping_core_simulator {
  skipping_core_simulator = 0
  separators = 0
}

/iOSSimulator: \[SimServiceContext sharedServiceContextForDeveloperDir:error:\] returned nil \(Error Domain=DVTCoreSimulatorAdditionsErrorDomain Code=3 "CoreSimulator is out of date\./ {
  next
}

{ print }
' "$log_file"

rm -f "$log_file"
exit "$status"
