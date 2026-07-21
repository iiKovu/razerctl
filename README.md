# BlackShark V3 Pro Xbox controls for Linux

A portable Linux control suite for the Razer BlackShark V3 Pro Xbox wireless
dongle (`1532:0a55`). It keeps the daemon, CLI, GUI, config, and logs inside this
repository. It does not install a systemd service or a tray application.

## Controls

- Battery percentage and charging state
- Five captured EQ presets: Default, Game, Movie, Music, and Counter-Strike 2
- Sidetone from 0–15
- THX Spatial Audio on/off
- ANC on/off and strength 1–4
- Automatic shutoff: off, 15, 30, 45, or 60 minutes
- Headset connection state for Polybar

The separate `razer-cfg255` DKMS module is still required for reliable Xbox
dongle enumeration. Adding PID `0a55` to userspace only makes the application
find the device; it does not enable the vendor control channel.

## Build the portable bundle

The repository includes source and portable binaries. To rebuild the three
binaries in place:

```bash
./install.sh
```

This only writes inside the repository. Runtime files are saved under `data/`.

Install the udev rule once so the active desktop user can open the HID node:

```bash
sudo install -m 0644 60-blackshark.rules /etc/udev/rules.d/60-blackshark.rules
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=hidraw
```

## DKMS compatibility module

The canonical source for the installed version is under
`dkms/razer-cfg255-1.0/`. It changes the first configuration-descriptor request
from Linux's 9 bytes to the 255 bytes expected by this Razer firmware.

One-time installation:

```bash
sudo cp -a dkms/razer-cfg255-1.0 /usr/src/
sudo dkms add razer-cfg255/1.0
sudo dkms install razer-cfg255/1.0
echo razer-cfg255 | sudo tee /etc/modules-load.d/razer-cfg255.conf
sudo modprobe razer-cfg255
```

`AUTOINSTALL="yes"` makes DKMS rebuild the module for future kernels. The
modules-load file makes it load at login-time boots; DKMS installation alone
does not load it into the running kernel.

The module must already be armed when the dongle enumerates. If the dongle was
enumerated before it loaded, re-enumerate once by reconnecting it or switching
USB → XBOX → USB. On machines whose built-in xHCI enumerates devices before
normal modules-load processing, a fully cold boot can still require this one
post-boot re-enumeration until the quirk is accepted into usbcore.

## Usage

```bash
./blackshark gui
./blackshark status
./blackshark diagnose
./blackshark ctl battery
./blackshark ctl eq 1
./blackshark ctl sidetone 8
./blackshark ctl thx on
./blackshark ctl anc on 4
./blackshark ctl power-savings 30
./blackshark start | stop | restart
```

The launcher starts the daemon when the GUI or CLI needs it. For a seamless
desktop setup, start `./blackshark start` once from the window-manager session;
the daemon remains lightweight and simply waits while the headset is absent.

`./blackshark diagnose` uses a checksum-correct battery reply as its independent
control-channel indicator and reports whether `razer-cfg255` is loaded. If the
dongle is in its audio-only fallback state, it gives the exact re-enumeration
step needed after the module is armed.

## Polybar

The helper prints nothing until the headset is connected. While connected it
shows battery, charging, and ANC state. Polybar itself only opens the GUI:

```ini
[module/blackshark]
type = custom/script
exec = /path/to/razer-control/polybar-blackshark status
interval = 15
interval-fail = 15
label = %output%
click-left = /path/to/razer-control/blackshark gui &
```

There are no click shortcuts that change headset settings.

## Updating

```bash
./blackshark update
```

This fast-forwards the maintained portable fork and rebuilds the local binaries.
It refuses to run over uncommitted changes. Upstream changes are first merged and
reviewed in this fork because upstream also ships tray, routing, and systemd
components that are intentionally excluded here.

For maintainers syncing upstream manually:

```bash
git fetch upstream
git merge upstream/master
./install.sh
```

## Architecture

`blacksharkd` is the only process that opens the HID control interface. It
validates report ID, status, transaction, response tuple, data size, and XOR
checksum, skips unsolicited notification frames, persists settings, and restores
them after reconnect. The CLI, GUI, and Polybar helper communicate with it over
the user session D-Bus name `net.blackshark1`.

Released under GPL-3.0; see [LICENSE](LICENSE).
