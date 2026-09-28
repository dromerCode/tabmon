# Manual testing

The automated tests cannot run niri, vkms or a tablet. Before each release, run through this list on a real setup (niri session, vkms loaded, tablet connected by USB) and tick every item.

## Checklist

- [ ] `tabmon doctor` shows every check as ✓ and exits with 0.
- [ ] `tabmon on` turns on the output, the tablet's VNC client connects to `localhost:5900` and shows the extended screen.
- [ ] `ss -ltnp | grep 5900` shows wayvnc listening on `127.0.0.1` only.
- [ ] `tabmon status` shows the output, port, device and wayvnc PID.
- [ ] `tabmon on` again says "already on".
- [ ] `tabmon off` stops wayvnc, turns the output off and says "tablet monitor off"; `adb reverse --list` no longer lists `tcp:5900`.
- [ ] `tabmon off` again says "already off".
- [ ] The keybinding (`tabmon toggle`) turns it on and off, with a notification each time.
- [ ] With the tablet unplugged, `tabmon on` fails with "no adb device found" and a notification, and the output stays off.
- [ ] With the session on, unplug the tablet and run `tabmon off`: it still turns the output off.
- [ ] With another program on port 5900 (`nc -l 127.0.0.1 5900`), `tabmon on` fails pointing to `wayvnc.log`, and the output is back off.
- [ ] Kill wayvnc by hand (`kill <pid>`), then `tabmon status` reports a stale session and `tabmon off` cleans it up.

## Release

1. Bump `version` in `Cargo.toml` and `pkgver` in `packaging/aur/PKGBUILD`, then run `cargo build` so `Cargo.lock` updates.
2. Run the checklist above.
3. Commit, tag `vX.Y.Z` and push the tag.
4. In `packaging/aur/`, run `updpkgsums`, then `makepkg -si` to check the package builds and installs.
5. Copy `PKGBUILD` to the AUR repository, run `makepkg --printsrcinfo > .SRCINFO`, commit and push.
