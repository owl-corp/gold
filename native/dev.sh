#!/usr/bin/env bash
# Try the working tree without touching the installed daemon or overlay.
#   native/dev.sh          build, (re)start a dev daemon and a dev overlay, bind Ctrl+D to the dev build
#   native/dev.sh stop     stop them and remove the bind
#   native/dev.sh logs     follow their logs
# While the dev daemon runs, `gold ctl` talks to it instead of the installed one; the installed
# overlay stays connected to the installed daemon and stays idle.
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")"
state="${XDG_RUNTIME_DIR:?}/gold-dev-pids"
gold="$PWD/target/debug/gold"

stop() {
  hyprctl eval 'if GOLD_DEV_BIND then GOLD_DEV_BIND:unbind(); GOLD_DEV_BIND = nil end' >/dev/null 2>&1 || true
  if [ -f "$state" ]; then
    # kill by pid, never by pattern (a pattern can match this shell)
    while read -r pid; do kill "$pid" 2>/dev/null || true; done <"$state"
    rm -f "$state"
  fi
}

# runs a command in its own session and records its pid (exec keeps the pid)
launch() {
  local log=$1; shift
  setsid sh -c 'echo $$ >>"$0"; exec "$@"' "$state" "$@" >"$log" 2>&1 </dev/null &
}

case "${1:-start}" in
  stop) stop; echo stopped ;;
  logs) tail -f /tmp/gold-dev-daemon.log /tmp/gold-dev-overlay.log ;;
  start)
    stop
    nix develop .. -c cargo build --quiet
    export GOLD_DEV=1
    launch /tmp/gold-dev-daemon.log "$gold" daemon
    (cd quickshell/dev && launch /tmp/gold-dev-overlay.log quickshell -p shell.qml)
    # the overlay retries its connection every 2 s
    for _ in $(seq 10); do
      status=$("$gold" ctl status 2>&1 || true)
      case "$status" in "ui clients: "[1-9]*) break ;; esac
      sleep 1
    done
    echo "$status"
    hyprctl eval "GOLD_DEV_BIND = hl.bind('CTRL + D', hl.dsp.exec_cmd('$gold ctl price-check'), { non_consuming = true })"
    ;;
  *) echo "usage: dev.sh [start|stop|logs]"; exit 1 ;;
esac
