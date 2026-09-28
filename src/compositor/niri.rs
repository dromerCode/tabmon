use std::collections::HashMap;

use anyhow::{Context, Result};
use serde::Deserialize;

use super::{Compositor, OutputState};
use crate::error::UserError;
use crate::system::System;

pub struct Niri<'a> {
    sys: &'a dyn System,
}

impl<'a> Niri<'a> {
    pub fn new(sys: &'a dyn System) -> Self {
        Self { sys }
    }
}

#[derive(Deserialize)]
struct NiriOutput {
    /// `null` while the output is off.
    logical: Option<serde_json::Value>,
}

/// Output names from `niri msg --json outputs`, mapped to whether they are on.
pub fn parse_outputs(json: &str) -> Result<HashMap<String, bool>> {
    let outputs: HashMap<String, NiriOutput> = serde_json::from_str(json)
        .context("could not parse the output of `niri msg --json outputs`")?;
    Ok(outputs
        .into_iter()
        .map(|(name, o)| (name, o.logical.is_some()))
        .collect())
}

impl Compositor for Niri<'_> {
    fn output_state(&self, output: &str) -> Result<OutputState> {
        let out = self.sys.run("niri", &["msg", "--json", "outputs"])?;
        if !out.success {
            return Err(
                UserError::new(format!("could not talk to niri: {}", out.stderr.trim()))
                    .hint("tabmon needs a running niri session")
                    .into(),
            );
        }
        Ok(match parse_outputs(&out.stdout)?.get(output) {
            None => OutputState::Missing,
            Some(false) => OutputState::Off,
            Some(true) => OutputState::On,
        })
    }

    fn set_output(&self, output: &str, on: bool) -> Result<()> {
        let action = if on { "on" } else { "off" };
        let out = self.sys.run("niri", &["msg", "output", output, action])?;
        if !out.success {
            return Err(UserError::new(format!(
                "niri could not turn {output} {action}: {}",
                out.stderr.trim()
            ))
            .into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compositor::{Compositor, OutputState};
    use crate::error::render;
    use crate::testing::{FakeSystem, fail, ok};

    const ON: &str = include_str!("../../tests/fixtures/niri-outputs-on.json");
    const OFF: &str = include_str!("../../tests/fixtures/niri-outputs-off.json");
    const MISSING: &str = include_str!("../../tests/fixtures/niri-outputs-missing.json");

    #[test]
    fn parses_real_outputs() {
        assert_eq!(parse_outputs(ON).unwrap().get("Virtual-1"), Some(&true));
        assert_eq!(parse_outputs(OFF).unwrap().get("Virtual-1"), Some(&false));
        assert_eq!(parse_outputs(MISSING).unwrap().get("Virtual-1"), None);
    }

    #[test]
    fn output_state_reads_niri() {
        let fake = FakeSystem::new();
        let niri = Niri::new(&fake);
        fake.respond("niri msg --json outputs", ok(OFF));
        fake.respond("niri msg --json outputs", ok(ON));
        fake.respond("niri msg --json outputs", ok(MISSING));
        assert_eq!(niri.output_state("Virtual-1").unwrap(), OutputState::Off);
        assert_eq!(niri.output_state("Virtual-1").unwrap(), OutputState::On);
        assert_eq!(
            niri.output_state("Virtual-1").unwrap(),
            OutputState::Missing
        );
    }

    #[test]
    fn niri_not_running_is_explained() {
        let fake = FakeSystem::new();
        fake.respond(
            "niri msg --json outputs",
            fail("Error: NIRI_SOCKET is not set\n"),
        );
        let text = render(&Niri::new(&fake).output_state("Virtual-1").unwrap_err());
        assert_eq!(
            text,
            "error: could not talk to niri: Error: NIRI_SOCKET is not set\nhint: tabmon needs a running niri session"
        );
    }

    #[test]
    fn set_output_calls_niri() {
        let fake = FakeSystem::new();
        let niri = Niri::new(&fake);
        niri.set_output("Virtual-1", true).unwrap();
        niri.set_output("Virtual-1", false).unwrap();
        assert_eq!(
            fake.calls(),
            vec![
                "niri msg output Virtual-1 on",
                "niri msg output Virtual-1 off"
            ]
        );
    }

    #[test]
    fn set_output_failure_is_reported() {
        let fake = FakeSystem::new();
        fake.respond(
            "niri msg output Virtual-1 on",
            fail("Error: output not found\n"),
        );
        let text = render(&Niri::new(&fake).set_output("Virtual-1", true).unwrap_err());
        assert_eq!(
            text,
            "error: niri could not turn Virtual-1 on: Error: output not found"
        );
    }
}
