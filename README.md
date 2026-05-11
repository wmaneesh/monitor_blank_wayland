# monitor_blank

A lightweight Wayland overlay utility for Hyprland/Linux that blanks selected monitors and optionally dims their brightness using `ddcutil`.


---

## Features

* Fullscreen black overlay using `wlr-layer-shell`
* Per-monitor brightness dimming via `ddcutil`
* Automatic brightness restore on exit
* Toggle behavior using a lockfile
* Multi-monitor support
* Works on Wayland compositors such as Hyprland
* `ESC` key closes overlays

---

## Demo

Example:

```bash
monitor_blank DP-1:0:60 DP-2:0:71
```

This will:

* Dim `DP-1` brightness to `0`
* Restore `DP-1` brightness to `60` when closed
* Dim `DP-2` brightness to `0`
* Restore `DP-2` brightness to `71` when closed

Pressing the keybind again toggles the overlay off and restores brightness.

---

## Requirements

* Linux
* Wayland compositor
* Hyprland recommended
* `ddcutil`
* DDC/CI enabled on your monitor

---

## Installing Dependencies

### Arch Linux

```bash
sudo pacman -S ddcutil
```

---

## Enable DDC/CI

Most monitors require DDC/CI to be enabled in the monitor OSD settings.

Example settings names:

* DDC/CI
* MCCS
* External Control

---

## Finding Monitor Names

Run:

```bash
hyprctl monitors
```

Example output:

```text
Monitor DP-1
Monitor DP-2
```

Use these names in the CLI arguments.

---

## Finding Restore Brightness Values

Set your monitor to your preferred brightness manually, then run:

```bash
ddcutil getvcp 10
```

Example:

```text
current value = 71
```

Use this as the restore brightness value.

---

## Usage

```bash
monitor_blank OUTPUT:DIM:RESTORE
```

### Multiple monitors

```bash
monitor_blank DP-1:0:60 DP-2:0:71
```

### Partial dimming

```bash
monitor_blank DP-2:10:71
```

Dims monitor to brightness `10` instead of full black.

---

## Hyprland Keybind Example

```ini
# monitor blank keybinds
bind = $mainMod, bracketleft, exec, monitor_blank DP-2:0:71
bind = $mainMod, bracketright, exec, monitor_blank DP-1:0:60 DP-2:0:71
```

Pressing the same keybind again will:

* Remove overlays
* Restore brightness
* Exit the running instance

---

## Build

```bash
cargo build --release
```

---

## Install Binary

Example:

```bash
sudo cp target/release/monitor_blank /usr/local/bin/
```

---

### Overlay appears but monitor is not dimmed

Some monitors do not support software brightness control through DDC/CI.

The overlay will still function normally.

---

## Version History

### v0.2.0

* Added brightness dimming support
* Added brightness restore on exit
* Added SIGTERM cleanup handling
* Added multi-monitor brightness configs

### v0.1.0

* Initial fullscreen black overlay release
* Close through `ESC` or triggering same keybind
