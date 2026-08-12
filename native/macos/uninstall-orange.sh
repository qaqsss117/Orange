#!/bin/sh
set -eu

if [ "$(/usr/bin/id -u)" -ne 0 ]; then
  echo "Orange uninstaller must run as root" >&2
  exit 1
fi

label="com.orangevpn.cn.helper"
domain="system/$label"
helper="/Library/PrivilegedHelperTools/$label"
socket="/var/run/com.orangevpn.cn.data-plane.sock"

/bin/launchctl bootout "$domain" >/dev/null 2>&1 || true
attempts=0
while [ "$attempts" -lt 30 ]; do
  if ! /bin/launchctl print "$domain" >/dev/null 2>&1; then
    if [ -S "$socket" ]; then
      if [ "$(/usr/bin/stat -f '%u' "$socket")" != "0" ]; then
        echo "Orange socket is not root-owned; uninstall was cancelled" >&2
        exit 1
      fi
      /bin/rm -f "$socket"
    fi
    [ ! -e "$socket" ] && break
  fi
  attempts=$((attempts + 1))
  /bin/sleep 1
done
if [ "$attempts" -ge 30 ]; then
  echo "Orange helper did not stop; uninstall was cancelled" >&2
  exit 1
fi
if [ -x "$helper" ]; then
  "$helper" --restore-proxy || true
fi

/bin/rm -f "/Library/LaunchDaemons/$label.plist"
/bin/rm -f "$helper" "/Library/PrivilegedHelperTools/orange-data-plane"
/bin/rm -rf \
  "/Library/Application Support/Orange" \
  "/Applications/Orange.app" \
  "/usr/local/share/orange"
exit 0
