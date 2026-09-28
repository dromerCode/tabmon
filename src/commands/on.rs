use std::time::Duration;

use anyhow::{Context, Result};

use super::{Ctx, active_state};
use crate::adb;
use crate::compositor::{OutputState, missing_output};
use crate::error::{UserError, with_rollback_failures};
use crate::notify;
use crate::state::{self, State};
use crate::vnc;

const WAIT_STEPS: u32 = 50;
const WAIT_STEP: Duration = Duration::from_millis(100);

/// A step already done that must be undone if a later one fails.
enum Undo {
    OutputOff,
    RemoveReverse,
    StopVnc(u32),
}

pub fn run(ctx: &Ctx) -> Result<String> {
    if active_state(ctx)?.is_some() {
        return Ok("already on".into());
    }
    let cfg = &ctx.config;
    let before = ctx.compositor.output_state(&cfg.output)?;
    if before == OutputState::Missing {
        return Err(missing_output(&cfg.output).into());
    }
    let serial = adb::select_device(&adb::devices(ctx.sys)?, cfg.adb_serial.as_deref())?;

    let mut done = Vec::new();
    if let Err(err) = bring_up(ctx, before, &serial, &mut done) {
        let failures = done
            .iter()
            .rev()
            .filter_map(|step| undo(ctx, &serial, step).err())
            .collect();
        return Err(with_rollback_failures(err, failures));
    }

    if cfg.notify {
        let body = format!("Connect the VNC client to localhost:{}", cfg.port);
        notify::send(ctx.sys, "Tablet monitor on", &body, false);
    }
    Ok(format!(
        "tablet monitor on ({}, localhost:{}, device {serial})",
        cfg.output, cfg.port
    ))
}

fn bring_up(ctx: &Ctx, before: OutputState, serial: &str, done: &mut Vec<Undo>) -> Result<()> {
    let cfg = &ctx.config;
    if before == OutputState::Off {
        ctx.compositor.set_output(&cfg.output, true)?;
        done.push(Undo::OutputOff);
        wait_until_on(ctx)?;
    }
    // A reverse that was already there belongs to someone else: never undo it.
    let created_reverse = !adb::has_reverse(ctx.sys, serial, cfg.port)?;
    adb::reverse(ctx.sys, serial, cfg.port)?;
    if created_reverse {
        done.push(Undo::RemoveReverse);
    }

    std::fs::create_dir_all(&ctx.paths.runtime_dir)
        .with_context(|| format!("could not create {}", ctx.paths.runtime_dir.display()))?;
    let pid = vnc::start(
        ctx.sys,
        &cfg.output,
        cfg.port,
        cfg.max_fps,
        &ctx.paths.vnc_log(),
    )?;
    done.push(Undo::StopVnc(pid));

    let saved = State {
        pid,
        port: cfg.port,
        serial: serial.to_string(),
        output: cfg.output.clone(),
        created_reverse,
    };
    state::save(&ctx.paths, &saved)
}

fn wait_until_on(ctx: &Ctx) -> Result<()> {
    let output = &ctx.config.output;
    for _ in 0..WAIT_STEPS {
        if ctx.compositor.output_state(output)? == OutputState::On {
            return Ok(());
        }
        ctx.sys.sleep(WAIT_STEP);
    }
    Err(
        UserError::new(format!("{output} did not turn on within 5 s"))
            .hint("check `niri msg outputs` and the output block in niri's config.kdl")
            .into(),
    )
}

fn undo(ctx: &Ctx, serial: &str, step: &Undo) -> Result<()> {
    match step {
        Undo::OutputOff => ctx.compositor.set_output(&ctx.config.output, false),
        Undo::RemoveReverse => adb::remove_reverse(ctx.sys, serial, ctx.config.port),
        Undo::StopVnc(pid) => vnc::stop(ctx.sys, *pid),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::tests_support::{ADB_ONE, MISSING, OFF, ON, ctx};
    use crate::compositor::niri::Niri;
    use crate::error::render;
    use crate::state::State;
    use crate::testing::{FakeSystem, SPAWN_PID, fail, ok, temp_paths};

    const NOTIFY_ON: &str =
        "notify-send --app-name=tabmon Tablet monitor on Connect the VNC client to localhost:5900";

    fn saved(pid: u32) -> State {
        State {
            pid,
            port: 5900,
            serial: "ABC123".into(),
            output: "Virtual-1".into(),
            created_reverse: true,
        }
    }

    /// niri reports the output off first, then on; one adb device.
    fn script_off_then_on(fake: &FakeSystem) {
        fake.respond("niri msg --json outputs", ok(OFF));
        fake.respond("niri msg --json outputs", ok(ON));
        fake.respond("adb devices", ok(ADB_ONE));
    }

    #[test]
    fn turns_everything_on_in_order() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        script_off_then_on(&fake);
        let niri = Niri::new(&fake);

        let message = run(&ctx(&fake, &niri, &paths)).unwrap();

        assert_eq!(
            message,
            "tablet monitor on (Virtual-1, localhost:5900, device ABC123)"
        );
        assert_eq!(
            fake.calls(),
            vec![
                "niri msg --json outputs",
                "adb devices",
                "niri msg output Virtual-1 on",
                "niri msg --json outputs",
                "adb -s ABC123 reverse --list",
                "adb -s ABC123 reverse tcp:5900 tcp:5900",
                "spawn wayvnc -r --max-fps=90 -o Virtual-1 127.0.0.1 5900",
                NOTIFY_ON,
            ]
        );
        assert_eq!(state::load(&paths).unwrap(), Some(saved(SPAWN_PID)));
    }

    #[test]
    fn already_on_does_nothing() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        state::save(&paths, &saved(77)).unwrap();
        fake.add_process(77, "wayvnc");
        let niri = Niri::new(&fake);

        assert_eq!(run(&ctx(&fake, &niri, &paths)).unwrap(), "already on");
        assert!(fake.calls().is_empty());
    }

    #[test]
    fn stale_state_does_not_block_on() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        state::save(&paths, &saved(77)).unwrap();
        script_off_then_on(&fake);
        let niri = Niri::new(&fake);

        run(&ctx(&fake, &niri, &paths)).unwrap();
        assert_eq!(state::load(&paths).unwrap(), Some(saved(SPAWN_PID)));
    }

    #[test]
    fn missing_output_is_an_error() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        fake.respond("niri msg --json outputs", ok(MISSING));
        let niri = Niri::new(&fake);

        let text = render(&run(&ctx(&fake, &niri, &paths)).unwrap_err());
        assert!(
            text.starts_with(
                "error: output Virtual-1 not found in niri\nhint: load the vkms module"
            ),
            "{text}"
        );
    }

    #[test]
    fn no_device_fails_before_touching_anything() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        fake.respond("niri msg --json outputs", ok(OFF));
        fake.respond("adb devices", ok("List of devices attached\n\n"));
        let niri = Niri::new(&fake);

        let text = render(&run(&ctx(&fake, &niri, &paths)).unwrap_err());
        assert!(text.starts_with("error: no adb device found"), "{text}");
        assert_eq!(fake.calls(), vec!["niri msg --json outputs", "adb devices"]);
    }

    #[test]
    fn output_that_never_turns_on_is_rolled_back() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        fake.respond("niri msg --json outputs", ok(OFF));
        fake.respond("adb devices", ok(ADB_ONE));
        let niri = Niri::new(&fake);

        let text = render(&run(&ctx(&fake, &niri, &paths)).unwrap_err());
        assert!(
            text.starts_with("error: Virtual-1 did not turn on within 5 s"),
            "{text}"
        );
        assert_eq!(
            fake.calls().last().unwrap(),
            "niri msg output Virtual-1 off"
        );
    }

    #[test]
    fn reverse_failure_turns_the_output_back_off() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        script_off_then_on(&fake);
        fake.respond(
            "adb -s ABC123 reverse tcp:5900 tcp:5900",
            fail("error: device offline"),
        );
        let niri = Niri::new(&fake);

        let text = render(&run(&ctx(&fake, &niri, &paths)).unwrap_err());
        assert!(text.starts_with("error: adb reverse failed"), "{text}");
        assert_eq!(
            fake.calls().last().unwrap(),
            "niri msg output Virtual-1 off"
        );
        assert_eq!(state::load(&paths).unwrap(), None);
    }

    #[test]
    fn wayvnc_dying_undoes_reverse_and_output() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        script_off_then_on(&fake);
        fake.set_spawn_dies(true);
        let niri = Niri::new(&fake);

        let text = render(&run(&ctx(&fake, &niri, &paths)).unwrap_err());
        assert!(
            text.contains("wayvnc exited right after starting"),
            "{text}"
        );
        assert!(text.contains("wayvnc.log"), "{text}");
        let calls = fake.calls();
        assert_eq!(
            calls[calls.len() - 2..],
            [
                "adb -s ABC123 reverse --remove tcp:5900",
                "niri msg output Virtual-1 off"
            ]
        );
        assert_eq!(state::load(&paths).unwrap(), None);
    }

    #[test]
    fn rollback_keeps_output_on_if_it_was_already_on() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        fake.respond("niri msg --json outputs", ok(ON));
        fake.respond("adb devices", ok(ADB_ONE));
        fake.set_spawn_dies(true);
        let niri = Niri::new(&fake);

        run(&ctx(&fake, &niri, &paths)).unwrap_err();
        let calls = fake.calls();
        assert!(
            !calls.iter().any(|c| c.starts_with("niri msg output")),
            "{calls:?}"
        );
        assert_eq!(
            calls.last().unwrap(),
            "adb -s ABC123 reverse --remove tcp:5900"
        );
    }

    #[test]
    fn rollback_keeps_a_reverse_that_already_existed() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        fake.respond("niri msg --json outputs", ok(ON));
        fake.respond("adb devices", ok(ADB_ONE));
        fake.respond(
            "adb -s ABC123 reverse --list",
            ok("UsbFfs tcp:5900 tcp:5900\n"),
        );
        fake.set_spawn_dies(true);
        let niri = Niri::new(&fake);

        run(&ctx(&fake, &niri, &paths)).unwrap_err();
        let calls = fake.calls();
        assert!(
            !calls.iter().any(|c| c.contains("reverse --remove")),
            "{calls:?}"
        );
    }

    #[test]
    fn existing_reverse_is_recorded_as_not_ours() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        script_off_then_on(&fake);
        fake.respond(
            "adb -s ABC123 reverse --list",
            ok("UsbFfs tcp:5900 tcp:5900\n"),
        );
        let niri = Niri::new(&fake);

        run(&ctx(&fake, &niri, &paths)).unwrap();
        assert!(!state::load(&paths).unwrap().unwrap().created_reverse);
    }

    #[test]
    fn rollback_failures_are_reported_too() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        script_off_then_on(&fake);
        fake.respond(
            "adb -s ABC123 reverse tcp:5900 tcp:5900",
            fail("error: device offline"),
        );
        fake.respond("niri msg output Virtual-1 off", fail("Error: boom"));
        let niri = Niri::new(&fake);

        let text = render(&run(&ctx(&fake, &niri, &paths)).unwrap_err());
        assert!(text.contains("adb reverse failed"), "{text}");
        assert!(
            text.contains(
                "undoing the previous steps also failed: niri could not turn Virtual-1 off"
            ),
            "{text}"
        );
    }

    #[test]
    fn notifications_can_be_disabled() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        script_off_then_on(&fake);
        let niri = Niri::new(&fake);
        let mut ctx = ctx(&fake, &niri, &paths);
        ctx.config.notify = false;

        run(&ctx).unwrap();
        assert!(!fake.calls().iter().any(|c| c.starts_with("notify-send")));
    }
}
