# tabmon

Use an Android tablet as an **extended** second monitor on [niri](https://github.com/YaLTeR/niri), over USB.

niri cannot create virtual outputs the way Hyprland (`hyprctl output create headless`) or Sway (`swaymsg create_output`) can. tabmon works around it with the kernel's `vkms` module, which adds a fake display that niri treats like a real one, and streams it to the tablet with wayvnc.

## How it works

1. `vkms` creates a virtual output (`Virtual-1`) that niri configures like any monitor.
2. `tabmon on` turns it on, runs `adb reverse tcp:5900 tcp:5900` and starts wayvnc on `127.0.0.1` for that output only.
3. A VNC client on the tablet connects to `localhost:5900`, which reaches the PC through the USB cable.

## Install

> **AUR package: work in progress.** We're still working on it, so `paru -S tabmon` won't work yet. For now, build it from source.

From the AUR (coming soon):

```sh
paru -S tabmon
```

From source (needs `niri`, `wayvnc` and `android-tools` installed):

```sh
cargo build --release
install -Dm755 target/release/tabmon ~/.local/bin/tabmon
```

## Setup

1. **Load vkms.** The AUR package will load it at boot once it is published. Until then, run `sudo modprobe vkms` and add `vkms` to a file in `/etc/modules-load.d/` to make it permanent.
2. **Configure the output in niri** (`~/.config/niri/config.kdl`). Adjust the mode to your tablet and the position to where it sits next to your screen:

   ```kdl
   output "Virtual-1" {
       off
       mode "2560x1600@59.987"
       scale 1.5
       position x=-2944 y=0
   }
   ```

3. **On the tablet:** enable USB debugging, connect the cable and accept the prompt. Install a VNC client (for example [AVNC](https://github.com/gujjwal00/avnc)) and add a server at `localhost`, port `5900`.
4. **Check everything:** `tabmon doctor`.
5. **Add a keybinding** in niri:

   ```kdl
   binds {
       Mod+Shift+T hotkey-overlay-title="Toggle Tablet Monitor" { spawn "tabmon" "toggle"; }
   }
   ```

## Usage

```
tabmon on       turn on the virtual output and start the connection
tabmon off      stop everything tabmon started
tabmon toggle   on or off depending on the current state
tabmon status   whether it is running, and on which output, port and device
tabmon doctor   check the setup and explain how to fix what is missing
```

Add `-v` to see every command tabmon runs.

## Configuration

Optional. `~/.config/tabmon/config.toml` (these are the defaults):

```toml
output  = "Virtual-1"   # vkms output defined in niri's config.kdl
port    = 5900
max_fps = 90
notify  = true          # desktop notifications on on/off and on errors
# adb_serial = "..."    # only needed with several adb devices connected
```

## Troubleshooting

Run `tabmon doctor`. Each failed check says how to fix it. If wayvnc fails to start, its output is in `$XDG_RUNTIME_DIR/tabmon/wayvnc.log`.

## Security

wayvnc only listens on `127.0.0.1` and the tablet reaches it through `adb reverse` over USB, so it is never exposed to your network.

It has no password, though: while it is on, **any local process or user on this machine** can connect to `127.0.0.1:5900` and control that screen, including sandboxed apps that share the host network. Turn it off (`tabmon off`) when you are not using it.

## Limitations

- Only niri for now (Hyprland and Sway can create headless outputs on their own).
- Only USB; Wi-Fi would need authentication and is not supported yet.

## License

MIT
