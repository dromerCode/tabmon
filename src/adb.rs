use anyhow::Result;

use crate::error::UserError;
use crate::system::System;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub serial: String,
    /// `device` when ready; otherwise `unauthorized`, `offline`, `no permissions (...)`...
    pub state: String,
}

/// Parses the output of `adb devices`.
pub fn parse_devices(text: &str) -> Vec<Device> {
    text.lines()
        .filter(|line| !line.starts_with('*') && !line.starts_with("List of devices"))
        .filter_map(|line| {
            let (serial, state) = line.split_once('\t')?;
            Some(Device {
                serial: serial.trim().to_string(),
                state: state.trim().to_string(),
            })
        })
        .collect()
}

pub fn devices(sys: &dyn System) -> Result<Vec<Device>> {
    let out = sys.run("adb", &["devices"])?;
    if !out.success {
        return Err(UserError::new(format!("adb devices failed: {}", out.stderr.trim())).into());
    }
    Ok(parse_devices(&out.stdout))
}

/// Picks the device to use: the configured serial, or the only ready device.
pub fn select_device(devices: &[Device], wanted: Option<&str>) -> Result<String, UserError> {
    if let Some(serial) = wanted {
        return match devices.iter().find(|d| d.serial == serial) {
            Some(d) if d.state == "device" => Ok(d.serial.clone()),
            Some(d) => Err(not_ready(d)),
            None => Err(UserError::new(format!("adb device {serial} not found"))
                .hint("check the USB cable, or fix adb_serial in ~/.config/tabmon/config.toml")),
        };
    }
    let ready: Vec<&Device> = devices.iter().filter(|d| d.state == "device").collect();
    match ready.as_slice() {
        [one] => Ok(one.serial.clone()),
        [] => Err(match devices.first() {
            Some(d) => not_ready(d),
            None => UserError::new("no adb device found")
                .hint("connect the tablet by USB and enable USB debugging"),
        }),
        many => {
            let serials = many
                .iter()
                .map(|d| d.serial.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            Err(
                UserError::new(format!("several adb devices connected: {serials}"))
                    .hint("set adb_serial in ~/.config/tabmon/config.toml"),
            )
        }
    }
}

fn not_ready(d: &Device) -> UserError {
    match d.state.as_str() {
        "unauthorized" => UserError::new(format!("adb device {} is unauthorized", d.serial))
            .hint("accept the USB debugging prompt on the tablet"),
        s if s.starts_with("no permissions") => {
            UserError::new(format!("no permission to access adb device {}", d.serial))
                .hint("install the android-udev package, then reconnect the tablet")
        }
        other => UserError::new(format!("adb device {} is {other}", d.serial))
            .hint("reconnect the tablet and try again"),
    }
}

pub fn reverse(sys: &dyn System, serial: &str, port: u16) -> Result<()> {
    let tcp = format!("tcp:{port}");
    let out = sys.run("adb", &["-s", serial, "reverse", &tcp, &tcp])?;
    if !out.success {
        return Err(
            UserError::new(format!("adb reverse failed: {}", out.stderr.trim()))
                .hint("reconnect the tablet and try again")
                .into(),
        );
    }
    Ok(())
}

pub fn remove_reverse(sys: &dyn System, serial: &str, port: u16) -> Result<()> {
    let tcp = format!("tcp:{port}");
    let out = sys.run("adb", &["-s", serial, "reverse", "--remove", &tcp])?;
    if !out.success {
        return Err(UserError::new(format!(
            "could not remove adb reverse {tcp}: {}",
            out.stderr.trim()
        ))
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::render;
    use crate::testing::{FakeSystem, fail, ok};

    fn dev(serial: &str, state: &str) -> Device {
        Device {
            serial: serial.into(),
            state: state.into(),
        }
    }

    #[test]
    fn parse_reads_devices() {
        let text = "List of devices attached\nABC123\tdevice\nXYZ\tunauthorized\n\n";
        assert_eq!(
            parse_devices(text),
            vec![dev("ABC123", "device"), dev("XYZ", "unauthorized")]
        );
    }

    #[test]
    fn parse_skips_daemon_messages() {
        let text = "* daemon not running; starting now at tcp:5037\n* daemon started successfully\nList of devices attached\nABC123\tdevice\n";
        assert_eq!(parse_devices(text), vec![dev("ABC123", "device")]);
    }

    #[test]
    fn parse_keeps_multi_word_states() {
        let text = "List of devices attached\nABC123\tno permissions (missing udev rules?); see [http://developer.android.com/tools/device.html]\n";
        assert!(parse_devices(text)[0].state.starts_with("no permissions"));
    }

    #[test]
    fn single_ready_device_is_selected() {
        assert_eq!(
            select_device(&[dev("ABC123", "device")], None).unwrap(),
            "ABC123"
        );
    }

    #[test]
    fn no_device_is_an_error() {
        let err = select_device(&[], None).unwrap_err();
        assert_eq!(err.message, "no adb device found");
        assert_eq!(
            err.hint.as_deref(),
            Some("connect the tablet by USB and enable USB debugging")
        );
    }

    #[test]
    fn unauthorized_device_gets_prompt_hint() {
        let err = select_device(&[dev("XYZ", "unauthorized")], None).unwrap_err();
        assert_eq!(err.message, "adb device XYZ is unauthorized");
        assert_eq!(
            err.hint.as_deref(),
            Some("accept the USB debugging prompt on the tablet")
        );
    }

    #[test]
    fn no_permissions_gets_udev_hint() {
        let err =
            select_device(&[dev("XYZ", "no permissions (missing udev rules?)")], None).unwrap_err();
        assert_eq!(err.message, "no permission to access adb device XYZ");
        assert_eq!(
            err.hint.as_deref(),
            Some("install the android-udev package, then reconnect the tablet")
        );
    }

    #[test]
    fn several_devices_need_a_serial() {
        let err = select_device(&[dev("A", "device"), dev("B", "device")], None).unwrap_err();
        assert_eq!(err.message, "several adb devices connected: A, B");
        assert_eq!(
            err.hint.as_deref(),
            Some("set adb_serial in ~/.config/tabmon/config.toml")
        );
    }

    #[test]
    fn configured_serial_is_selected() {
        let devices = [dev("A", "device"), dev("B", "device")];
        assert_eq!(select_device(&devices, Some("B")).unwrap(), "B");
    }

    #[test]
    fn configured_serial_must_be_connected() {
        let err = select_device(&[dev("A", "device")], Some("B")).unwrap_err();
        assert_eq!(err.message, "adb device B not found");
    }

    #[test]
    fn configured_serial_must_be_ready() {
        let err = select_device(&[dev("B", "offline")], Some("B")).unwrap_err();
        assert_eq!(err.message, "adb device B is offline");
        assert_eq!(
            err.hint.as_deref(),
            Some("reconnect the tablet and try again")
        );
    }

    #[test]
    fn devices_runs_adb() {
        let fake = FakeSystem::new();
        fake.respond(
            "adb devices",
            ok("List of devices attached\nABC123\tdevice\n"),
        );
        assert_eq!(devices(&fake).unwrap(), vec![dev("ABC123", "device")]);
    }

    #[test]
    fn reverse_and_remove_use_the_serial_and_port() {
        let fake = FakeSystem::new();
        reverse(&fake, "ABC123", 5900).unwrap();
        remove_reverse(&fake, "ABC123", 5900).unwrap();
        assert_eq!(
            fake.calls(),
            vec![
                "adb -s ABC123 reverse tcp:5900 tcp:5900",
                "adb -s ABC123 reverse --remove tcp:5900"
            ]
        );
    }

    #[test]
    fn reverse_failure_is_reported() {
        let fake = FakeSystem::new();
        fake.respond(
            "adb -s ABC123 reverse tcp:5900 tcp:5900",
            fail("error: device offline\n"),
        );
        let text = render(&reverse(&fake, "ABC123", 5900).unwrap_err());
        assert_eq!(
            text,
            "error: adb reverse failed: error: device offline\nhint: reconnect the tablet and try again"
        );
    }
}
