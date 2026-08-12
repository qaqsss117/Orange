#!/bin/sh
set -eu

if [ "$(/usr/bin/id -u)" -ne 0 ]; then
  echo "Orange uninstaller must run as root" >&2
  exit 1
fi

label="com.orangevpn.cn.helper"
helper="/Library/PrivilegedHelperTools/$label"

/bin/launchctl bootout "system/$label" >/dev/null 2>&1 || true
if [ -x "$helper" ]; then
  "$helper" --restore-proxy || true
fi

/bin/rm -f "/Library/LaunchDaemons/$label.plist"
/bin/rm -f "$helper" "/Library/PrivilegedHelperTools/orange-data-plane"
/bin/rm -rf "/Library/Application Support/Orange" "/Applications/Orange.app"
exit 0
