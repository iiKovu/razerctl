#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

not_contains() {
    local pattern="$1" file="$2"
    if grep -qE "$pattern" "$file"; then
        echo "unexpected '$pattern' in $file" >&2
        return 1
    fi
}

not_contains 'blackshark-tray' "$ROOT/Cargo.toml"
not_contains 'toggle-anc|click-right' "$ROOT/polybar-blackshark"
not_contains 'systemctl|blackshark-tray' "$ROOT/install.sh"
not_contains 'is not supported on the Xbox edition' "$ROOT/blackshark"

grep -q 'BLACKSHARK_DATA_DIR' "$ROOT/blackshark"
grep -q 'update.sh' "$ROOT/blackshark"
grep -q 'pull --ff-only' "$ROOT/update.sh"
grep -q '1532.*0a55' "$ROOT/60-blackshark.rules"
grep -q '0x0a55' "$ROOT/dkms/razer-cfg255-1.0/razer-cfg255.c"
grep -q 'AUTOINSTALL="yes"' "$ROOT/dkms/razer-cfg255-1.0/dkms.conf"

assert_line() {
    local expected="$1" file="$2"
    grep -qxF "$expected" "$file" || {
        echo "expected '$expected' in $file" >&2
        return 1
    }
}

make_gui_fixture() {
    local bundle="$1"
    mkdir -p "$bundle/bin" "$bundle/data"
    cp "$ROOT/blackshark" "$bundle/blackshark"
    chmod +x "$bundle/blackshark"

    cat >"$bundle/bin/busctl" <<'EOF'
#!/usr/bin/env bash
echo 'b true'
EOF
    chmod +x "$bundle/bin/busctl"
}

TMP_ROOT="$(mktemp -d)"
trap 'rm -rf "$TMP_ROOT"' EXIT

# A mapped GUI must be focused without starting another process.
FOCUS_FIXTURE="$TMP_ROOT/focus"
make_gui_fixture "$FOCUS_FIXTURE"
cat >"$FOCUS_FIXTURE/bin/xprop" <<'EOF'
#!/usr/bin/env bash
if [[ " $* " == *" -root "* ]]; then
    echo '_NET_CLIENT_LIST(WINDOW): window id # 0x1200007'
else
    echo 'WM_CLASS(STRING) = "blackshark-gui", "blackshark-gui"'
fi
EOF
cat >"$FOCUS_FIXTURE/bin/bspc" <<'EOF'
#!/usr/bin/env bash
echo "$*" >>"$TEST_LOG"
EOF
cat >"$FOCUS_FIXTURE/blackshark-gui" <<'EOF'
#!/usr/bin/env bash
echo launched >>"$TEST_LAUNCH_LOG"
EOF
chmod +x "$FOCUS_FIXTURE/bin/xprop" "$FOCUS_FIXTURE/bin/bspc" \
    "$FOCUS_FIXTURE/blackshark-gui"

TEST_LOG="$FOCUS_FIXTURE/focus.log" \
TEST_LAUNCH_LOG="$FOCUS_FIXTURE/launch.log" \
BLACKSHARK_DATA_DIR="$FOCUS_FIXTURE/data" \
PATH="$FOCUS_FIXTURE/bin:$PATH" \
    "$FOCUS_FIXTURE/blackshark" gui
assert_line 'node 0x1200007 -f' "$FOCUS_FIXTURE/focus.log"
[[ ! -e "$FOCUS_FIXTURE/launch.log" ]] || {
    echo "mapped GUI was launched again" >&2
    exit 1
}

# The lock must close the race before the first GUI maps its X11 window.
RACE_FIXTURE="$TMP_ROOT/race"
make_gui_fixture "$RACE_FIXTURE"
cat >"$RACE_FIXTURE/bin/xprop" <<'EOF'
#!/usr/bin/env bash
exit 1
EOF
cat >"$RACE_FIXTURE/blackshark-gui" <<'EOF'
#!/usr/bin/env bash
echo launched >>"$TEST_LAUNCH_LOG"
: >"$TEST_READY"
sleep 2
EOF
chmod +x "$RACE_FIXTURE/bin/xprop" "$RACE_FIXTURE/blackshark-gui"

TEST_LAUNCH_LOG="$RACE_FIXTURE/launch.log" \
TEST_READY="$RACE_FIXTURE/ready" \
BLACKSHARK_DATA_DIR="$RACE_FIXTURE/data" \
PATH="$RACE_FIXTURE/bin:$PATH" \
    "$RACE_FIXTURE/blackshark" gui &
FIRST_GUI_PID=$!
for _ in $(seq 1 50); do
    [[ -e "$RACE_FIXTURE/ready" ]] && break
    sleep 0.02
done
[[ -e "$RACE_FIXTURE/ready" ]] || {
    echo "first GUI fixture did not start" >&2
    kill "$FIRST_GUI_PID" 2>/dev/null || true
    exit 1
}

TEST_LAUNCH_LOG="$RACE_FIXTURE/launch.log" \
TEST_READY="$RACE_FIXTURE/ready" \
BLACKSHARK_DATA_DIR="$RACE_FIXTURE/data" \
PATH="$RACE_FIXTURE/bin:$PATH" \
    "$RACE_FIXTURE/blackshark" gui
wait "$FIRST_GUI_PID"

[[ "$(wc -l <"$RACE_FIXTURE/launch.log")" -eq 1 ]] || {
    echo "rapid GUI launches started more than one process" >&2
    exit 1
}
