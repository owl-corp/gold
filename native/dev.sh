#!/usr/bin/env bash
# Try the working tree without touching the installed daemon or overlay.
#   native/dev.sh          build, (re)start a dev daemon and a dev overlay
#   native/dev.sh stop     stop them
#   native/dev.sh logs     follow their logs
# While the dev daemon runs, `gold ctl` (so your Hyprland binds) talks to it instead of the
# installed one; the installed overlay stays connected to the installed daemon and stays idle.
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")"
state="${XDG_RUNTIME_DIR:?}/gold-dev-pids"

stop() {
  if [ -f "$state" ]; then
    # kill by pid, never by pattern (a pattern can match this shell)
    while read -r pid; do kill "$pid" 2>/dev/null || true; done <"$state"
    rm -f "$state"
  fi
}

case "${1:-start}" in
  stop) stop; echo stopped ;;
  logs) tail -f /tmp/gold-dev-daemon.log /tmp/gold-dev-overlay.log ;;
  start)
    stop
    nix develop .. -c cargo build --quiet
    export GOLD_DEV=1
    setsid target/debug/gold daemon >/tmp/gold-dev-daemon.log 2>&1 & echo $! >"$state"
    (cd quickshell/dev && setsid quickshell -p shell.qml >/tmp/gold-dev-overlay.log 2>&1 & echo $! >>"$state")
    # the overlay retries its connection every 2 s
    for _ in $(seq 10); do
      status=$(target/debug/gold ctl status 2>&1 || true)
      [ "$status" != "ui clients: 0" ] && break
      sleep 1
    done
    echo "$status"
    ;;
  *) echo "usage: dev.sh [start|stop|logs]"; exit 1 ;;
esac
