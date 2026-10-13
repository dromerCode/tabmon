use anyhow::{Context, Result};
use serde::Deserialize;

use super::{Compositor, Kind, OutputState};
use crate::error::UserError;
use crate::system::System;

/// Hyprland creates virtual outputs itself, so here "off" means "does not exist".
pub struct Hyprland<'a> {
    sys: &'a dyn System,
}

impl<'a> Hyprland<'a> {
    pub fn new(sys: &'a dyn System) -> Self {
        Self { sys }
    }
}

#[derive(Deserialize)]
struct Monitor {
    name: String,
}

/// Output names from `hyprctl -j monitors` (enabled outputs only).
pub fn parse_monitors(json: &str) -> Result<Vec<String>> {
    let monitors: Vec<Monitor> = serde_json::from_str(json)
        .context("could not parse the output of `hyprctl -j monitors`")?;
    Ok(monitors.into_iter().map(|m| m.name).collect())
}

impl Compositor for Hyprland<'_> {
    fn kind(&self) -> Kind {
        Kind::Hyprland
    }

    fn output_state(&self, output: &str) -> Result<OutputState> {
        let out = self.sys.run("hyprctl", &["-j", "monitors"])?;
        if !out.success {
            return Err(UserError::new(format!(
                "could not talk to Hyprland: {}",
                reply(&out.stdout, &out.stderr)
            ))
            .hint("tabmon needs a running Hyprland session")
            .into());
        }
        Ok(
            if parse_monitors(&out.stdout)?.iter().any(|m| m == output) {
                OutputState::On
            } else {
                OutputState::Off
            },
        )
    }

    fn set_output(&self, output: &str, on: bool) -> Result<()> {
        let action = if on { "create" } else { "remove" };
        let out = if on {
            self.sys
                .run("hyprctl", &["output", "create", "headless", output])?
        } else {
            self.sys.run("hyprctl", &["output", "remove", output])?
        };
        // hyprctl answers "ok", and may exit with 0 even when it failed.
        if !out.success || out.stdout.trim() != "ok" {
            return Err(UserError::new(format!(
                "Hyprland could not {action} output {output}: {}",
                reply(&out.stdout, &out.stderr)
            ))
            .into());
        }
        Ok(())
    }

    // Never reached: outputs that do not exist are created by `set_output`.
    fn missing_output(&self, output: &str) -> UserError {
        UserError::new(format!("output {output} not found in Hyprland"))
    }
}

/// hyprctl writes its errors to stdout or stderr depending on the version.
fn reply(stdout: &str, stderr: &str) -> String {
    let stderr = stderr.trim();
    if stderr.is_empty() {
        stdout.trim().to_string()
    } else {
        stderr.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::render;
    use crate::testing::{FakeSystem, fail, ok};

    const ON: &str = include_str!("../../tests/fixtures/hyprland-monitors-on.json");
    const OFF: &str = include_str!("../../tests/fixtures/hyprland-monitors-off.json");

    #[test]
    fn parses_real_monitors() {
        assert_eq!(parse_monitors(ON).unwrap(), vec!["eDP-1", "TABMON"]);
        assert_eq!(parse_monitors(OFF).unwrap(), vec!["eDP-1"]);
    }

    #[test]
    fn output_state_reads_hyprctl() {
        let fake = FakeSystem::new();
        let hypr = Hyprland::new(&fake);
        fake.respond("hyprctl -j monitors", ok(OFF));
        fake.respond("hyprctl -j monitors", ok(ON));
        assert_eq!(hypr.output_state("TABMON").unwrap(), OutputState::Off);
        assert_eq!(hypr.output_state("TABMON").unwrap(), OutputState::On);
    }

    #[test]
    fn hyprland_not_running_is_explained() {
        let fake = FakeSystem::new();
        fake.respond(
            "hyprctl -j monitors",
            fail("HYPRLAND_INSTANCE_SIGNATURE not set! (is hyprland running?)\n"),
        );
        let text = render(&Hyprland::new(&fake).output_state("TABMON").unwrap_err());
        assert_eq!(
            text,
            "error: could not talk to Hyprland: HYPRLAND_INSTANCE_SIGNATURE not set! (is hyprland running?)\nhint: tabmon needs a running Hyprland session"
        );
    }

    #[test]
    fn set_output_creates_and_removes_a_headless_output() {
        let fake = FakeSystem::new();
        fake.respond("hyprctl output create headless TABMON", ok("ok"));
        fake.respond("hyprctl output remove TABMON", ok("ok"));
        let hypr = Hyprland::new(&fake);
        hypr.set_output("TABMON", true).unwrap();
        hypr.set_output("TABMON", false).unwrap();
        assert_eq!(
            fake.calls(),
            vec![
                "hyprctl output create headless TABMON",
                "hyprctl output remove TABMON"
            ]
        );
    }

    #[test]
    fn hyprctl_answering_other_than_ok_is_a_failure() {
        // hyprctl can exit with 0 and still report an error on stdout.
        let fake = FakeSystem::new();
        fake.respond(
            "hyprctl output remove TABMON",
            ok("could not find output TABMON"),
        );
        let text = render(
            &Hyprland::new(&fake)
                .set_output("TABMON", false)
                .unwrap_err(),
        );
        assert_eq!(
            text,
            "error: Hyprland could not remove output TABMON: could not find output TABMON"
        );
    }
}
