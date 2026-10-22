#!/bin/bash
# SPDX-License-Identifier: GPL-3.0-or-later
# A stand-in for the install script that speaks its machine interface: `check` and `disks` print
# captured output; `install` and `mok-import` read the answers on descriptor 4 and write events on
# descriptor 3, as the real script does. STUB picks what it does, STUB_DIR is where it leaves what it
# saw, STUB_TTY=1 makes the handoffs use /dev/tty (they need a terminal of their own).
set -u
here="$(cd "$(dirname "$0")" && pwd)"
# The installer gives its children a clean environment: a test that runs the installer itself leaves
# what the stand-in needs in a file next to it.
# shellcheck disable=SC1091
[ ! -f "$here/stub.conf" ] || . "$here/stub.conf"
fx="${STUB_FX:-$here/../fixtures}"
what="${STUB:-ok}"

case "${1:-}" in
  check)
    [ -z "${STUB_DIR:-}" ] || { ls "/proc/$$/fd" > "$STUB_DIR/check-fds"; env > "$STUB_DIR/check-env"; }
    case "$what" in
      check-fails) echo "nyra-install: the tree is not the release it says" >&2; exit 1 ;;
      check-garbage) echo "this is not json"; exit 0 ;;
      check-huge) head -c 3000000 /dev/zero | tr '\0' ' '; exit 0 ;;
      check-hangs) sleep 30; exit 0 ;;
      check-protocol-2) sed 's/"protocol": 1/"protocol": 2/' "$fx/check-ok.json"; exit 0 ;;
      check-no-tpm) cat "$fx/check-no-tpm.json"; exit 0 ;;
      check-bad-fingerprint) sed 's/:F9"/:F9:00"/' "$fx/check-ok.json"; exit 0 ;;
    esac
    cat "$fx/check-ok.json"
    exit 0
    ;;
  disks)
    case "$what" in
      disks-fails) echo "nyra-install: cannot list the disks" >&2; exit 1 ;;
      disks-twins)
        # Two disks that nothing tells apart.
        sed 's/"name": "vda"/"name": "vdb"/; s|/dev/vda|/dev/vdb|' "$fx/disks-one.json" > "${STUB_DIR:?}/second.json"
        head -n -1 "$fx/disks-one.json"
        echo ","
        tail -n +2 "$STUB_DIR/second.json"
        exit 0
        ;;
      disks-control)
        sed 's/QEMU HARDDISK/QEMU\\u001b[2J\\u0007\\u009b HARD\\u202eDISK/' "$fx/disks-one.json"
        exit 0
        ;;
    esac
    cat "$fx/disks-one.json"
    exit 0
    ;;
  install | mok-import) sub="$1" ;;
  *) echo "usage" >&2; exit 1 ;;
esac

shift
[ "$*" = "--events-fd 3 --answers-fd 4" ] || { echo "arguments: $*" >&2; exit 1; }
emit() { printf '%s\n' "$1" >&3; }
ev() { emit "{\"event\":\"$1\"}"; }
step() { emit "{\"event\":\"step\",\"id\":\"$1\",\"state\":\"$2\"}"; }
fail() { emit "{\"event\":\"fail\",\"id\":\"$1\",\"disk_changed\":$2}"; exit "$3"; }
# Which descriptors the script was given: only the terminal, the two pipes and its own.
ls "/proc/$$/fd" > "${STUB_DIR:?}/fds"
env > "$STUB_DIR/env"

if [ "$sub" = install ]; then
  IFS= read -r -n 4097 -u 4 line || exit 1
  printf '%s\n' "$line" > "$STUB_DIR/answers"
fi

# The terminal, for a handoff: show the key and read it back, as the real script does.
handoff() { # handoff ID
  emit "{\"event\":\"handoff\",\"id\":\"$1\"}"
  IFS= read -r -u 4 go || exit 11
  [ "$go" = go ] || exit 11
  if [ "${STUB_TTY:-0}" = 1 ]; then
    if [ "$1" = recovery-key ]; then
      printf 'RECOVERY KEY: %s\nType it again: ' "$STUB_KEY" > /dev/tty
      IFS= read -r typed < /dev/tty
      printf '%s\n' "$typed" > "$STUB_DIR/typed"
    else
      printf 'MOK PASSWORD: ' > /dev/tty
      IFS= read -r typed < /dev/tty
      printf '%s\n' "$typed" > "$STUB_DIR/typed-mok"
    fi
    sleep "${STUB_AFTER:-0}"
    printf '\033[H\033[2J\033[3J' > /dev/tty
  fi
  ev handoff-end
}

if [ "$sub" = mok-import ]; then
  case "$what" in
    mok-slow)
      # A script that cleans up when it is told to stop, and takes a moment to do it.
      trap 'sleep 1; echo cleaned > "$STUB_DIR/cleaned"; exit 10' TERM
      # (It gives up by itself after a while, so that a test that fails does not leave it running.)
      for _ in $(seq 300); do sleep 0.05; done
      exit 1
      ;;
    mok-fail) emit '{"event":"handoff","id":"mok-password"}'; IFS= read -r -u 4 go; fail mok false 10 ;;
    mok-hostile) emit '{"event":"step","id":"verify","state":"start"}'; sleep 1; exit 0 ;;
  esac
  handoff mok-password
  ev done
  exit 0
fi

case "$what" in
  fail-before) step verify start; fail verify false 10 ;;
  memory) step verify start; fail memory false 10 ;;
  early-exit) exit 0 ;;
  cancel)
    trap 'fail verify false 10' TERM
    step verify start
    while :; do sleep 0.05; done
    ;;
  stubborn)
    # Does not stop when it is told to, like a script in the middle of a long command.
    trap '' TERM
    step verify start
    sleep 3
    exit 10
    ;;
  unknown) step verify start; emit '{"event":"launch"}'; sleep 5; exit 1 ;;
  huge-line)
    step verify start
    printf '%s' '{"event":"step","id":"' >&3
    head -c 200000 /dev/zero | tr '\0' 'x' >&3
    printf '"}\n' >&3
    sleep 5
    exit 1
    ;;
  control)
    step verify start
    printf '{"event":"disk-changed"\033[2J\007}\n' >&3
    sleep 5
    exit 1
    ;;
  out-of-order) step prepare start; sleep 5; exit 1 ;;
  flood)
    step verify start
    while :; do ev disk-changed || break; done
    exit 1
    ;;
esac

step verify start
step verify done
step prepare start
ev disk-changed
case "$what" in
  fail-after) fail write true 11 ;;
  fail-lie) fail write false 10 ;;
  fail-lie-exit) fail write false 11 ;;
  fail-says-ok-exits-0) emit '{"event":"fail","id":"write","disk_changed":true}'; exit 0 ;;
  die) kill -9 $$ ;;
  die-quietly) exit 1 ;;
esac
step prepare done
step install start
step install done
step protect start
handoff recovery-key
step protect done
step final start
handoff mok-password
case "$what" in
  mok-fails) fail mok true 11 ;;
esac
step final done
case "$what" in
  done-nonzero) ev done; exit 1 ;;
  extra-after-done) ev done; ev done; exit 0 ;;
  done-without-exit) ev done; sleep 1; exit 0 ;;
  done-exit-10) ev done; exit 10 ;;
esac
ev done
exit 0
