use super::Ctx;
use crate::adb;
use crate::compositor::{Kind, OutputState};
use crate::error::UserError;

pub struct Check {
    pub label: String,
    /// What is wrong and how to fix it; `None` when the check passed.
    pub problem: Option<String>,
}

pub struct Report {
    pub checks: Vec<Check>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.checks.iter().all(|c| c.problem.is_none())
    }

    pub fn render(&self) -> String {
        self.checks
            .iter()
            .map(|c| match &c.problem {
                None => format!("✓ {}", c.label),
                Some(problem) => format!("✗ {}\n  {problem}", c.label),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub fn run(ctx: &Ctx) -> Report {
    let mut checks = Vec::new();

    let kind = ctx.compositor.kind();
    let compositor_cli = match kind {
        Kind::Niri => ("niri", "--version"),
        Kind::Hyprland => ("hyprctl", "version"),
    };
    for (program, arg) in [compositor_cli, ("wayvnc", "--version"), ("adb", "version")] {
        let problem = ctx.sys.run(program, &[arg]).err().map(|e| describe(&e));
        checks.push(Check {
            label: format!("{program} installed"),
            problem,
        });
    }

    let output = &ctx.config.output;
    let output_state = ctx.compositor.output_state(output);
    match kind {
        // niri needs vkms for the output to exist at all.
        Kind::Niri => {
            checks.push(Check {
                label: "vkms module loaded".into(),
                problem: (!ctx.sys.module_loaded("vkms")).then(|| {
                    "sudo modprobe vkms; to load it at boot, copy tabmon-vkms.conf to /etc/modules-load.d/ (see the README)".into()
                }),
            });
            let problem = match output_state {
                Ok(OutputState::Missing) => {
                    Some(describe(&ctx.compositor.missing_output(output).into()))
                }
                Ok(_) => None,
                Err(e) => Some(describe(&e)),
            };
            checks.push(Check {
                label: format!("output {output} exists in niri"),
                problem,
            });
        }
        // Hyprland creates the output on `on`; it only has to answer.
        Kind::Hyprland => checks.push(Check {
            label: "Hyprland is reachable".into(),
            problem: output_state.err().map(|e| describe(&e)),
        }),
    }

    let device = adb::devices(ctx.sys).and_then(|devices| {
        Ok(adb::select_device(
            &devices,
            ctx.config.adb_serial.as_deref(),
        )?)
    });
    checks.push(match device {
        Ok(serial) => Check {
            label: format!("adb device ready ({serial})"),
            problem: None,
        },
        Err(e) => Check {
            label: "adb device ready".into(),
            problem: Some(describe(&e)),
        },
    });

    Report { checks }
}

/// One line with the problem and, when there is one, the fix.
fn describe(err: &anyhow::Error) -> String {
    match err.downcast_ref::<UserError>() {
        Some(UserError {
            message,
            hint: Some(hint),
        }) => format!("{message}: {hint}"),
        Some(UserError {
            message,
            hint: None,
        }) => message.clone(),
        None => format!("{err:#}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::tests_support::{ADB_ONE, MISSING, ON, ctx};
    use crate::compositor::hyprland::Hyprland;
    use crate::compositor::niri::Niri;
    use crate::error::UserError;
    use crate::testing::{FakeSystem, fail, ok, temp_paths};

    const HYPR_MONITORS: &str = include_str!("../../tests/fixtures/hyprland-monitors-off.json");

    fn healthy(fake: &FakeSystem) {
        fake.load_module("vkms");
        fake.respond("niri msg --json outputs", ok(ON));
        fake.respond("adb devices", ok(ADB_ONE));
    }

    #[test]
    fn all_checks_pass_on_a_healthy_system() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        healthy(&fake);
        let niri = Niri::new(&fake);

        let report = run(&ctx(&fake, &niri, &paths));
        assert!(report.ok());
        assert_eq!(
            report.render(),
            "✓ niri installed\n✓ wayvnc installed\n✓ adb installed\n✓ vkms module loaded\n✓ output Virtual-1 exists in niri\n✓ adb device ready (ABC123)"
        );
    }

    #[test]
    fn missing_program_shows_pacman_hint() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        healthy(&fake);
        fake.respond_err(
            "wayvnc --version",
            UserError::new("wayvnc not found").hint("pacman -S wayvnc"),
        );
        let niri = Niri::new(&fake);

        let report = run(&ctx(&fake, &niri, &paths));
        assert!(!report.ok());
        assert!(
            report
                .render()
                .contains("✗ wayvnc installed\n  wayvnc not found: pacman -S wayvnc"),
            "{}",
            report.render()
        );
    }

    #[test]
    fn missing_vkms_and_output_are_explained() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        fake.respond("niri msg --json outputs", ok(MISSING));
        fake.respond("adb devices", ok(ADB_ONE));
        let niri = Niri::new(&fake);

        let text = run(&ctx(&fake, &niri, &paths)).render();
        assert!(
            text.contains("✗ vkms module loaded\n  sudo modprobe vkms; to load it at boot, copy tabmon-vkms.conf to /etc/modules-load.d/ (see the README)"),
            "{text}"
        );
        assert!(text.contains("✗ output Virtual-1 exists in niri\n  output Virtual-1 not found in niri: load the vkms module"), "{text}");
    }

    #[test]
    fn unauthorized_device_is_explained() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        fake.load_module("vkms");
        fake.respond("niri msg --json outputs", ok(ON));
        fake.respond(
            "adb devices",
            ok("List of devices attached\nXYZ\tunauthorized\n"),
        );
        let niri = Niri::new(&fake);

        let text = run(&ctx(&fake, &niri, &paths)).render();
        assert!(
            text.ends_with("✗ adb device ready\n  adb device XYZ is unauthorized: accept the USB debugging prompt on the tablet"),
            "{text}"
        );
    }

    #[test]
    fn hyprland_skips_the_vkms_checks() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        fake.respond("hyprctl -j monitors", ok(HYPR_MONITORS));
        fake.respond("adb devices", ok(ADB_ONE));
        let hypr = Hyprland::new(&fake);

        let report = run(&ctx(&fake, &hypr, &paths));
        assert!(report.ok(), "{}", report.render());
        assert_eq!(
            report.render(),
            "✓ hyprctl installed\n✓ wayvnc installed\n✓ adb installed\n✓ Hyprland is reachable\n✓ adb device ready (ABC123)"
        );
    }

    #[test]
    fn unreachable_hyprland_is_explained() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        fake.respond(
            "hyprctl -j monitors",
            fail("couldn't connect to the socket\n"),
        );
        fake.respond("adb devices", ok(ADB_ONE));
        let hypr = Hyprland::new(&fake);

        let text = run(&ctx(&fake, &hypr, &paths)).render();
        assert!(
            text.contains("✗ Hyprland is reachable\n  could not talk to Hyprland: couldn't connect to the socket: tabmon needs a running Hyprland session"),
            "{text}"
        );
    }
}
