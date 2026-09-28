use anyhow::Result;

use super::Ctx;
use crate::adb;
use crate::compositor::OutputState;
use crate::notify;
use crate::state;
use crate::vnc;

pub fn run(ctx: &Ctx) -> Result<String> {
    let Some(saved) = state::load(&ctx.paths)? else {
        return Ok("already off".into());
    };
    let live = vnc::is_running(ctx.sys, saved.pid);
    if live {
        vnc::stop(ctx.sys, saved.pid)?;
        // The reverse disappears by itself when the tablet is unplugged, so failing here is fine.
        let _ = adb::remove_reverse(ctx.sys, &saved.serial, saved.port);
    }
    let output_off = turn_output_off(ctx, &saved.output);
    if live {
        output_off?;
    }
    state::clear(&ctx.paths)?;

    if !live {
        return Ok("already off (cleaned up a stale session)".into());
    }
    if ctx.config.notify {
        notify::send(
            ctx.sys,
            "Tablet monitor off",
            "The tablet display is disconnected",
            false,
        );
    }
    Ok("tablet monitor off".into())
}

fn turn_output_off(ctx: &Ctx, output: &str) -> Result<()> {
    if ctx.compositor.output_state(output)? == OutputState::On {
        ctx.compositor.set_output(output, false)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::tests_support::{OFF, ON, ctx};
    use crate::compositor::niri::Niri;
    use crate::state::State;
    use crate::testing::{FakeSystem, fail, ok, temp_paths};

    fn saved() -> State {
        State {
            pid: 77,
            port: 5900,
            serial: "ABC123".into(),
            output: "Virtual-1".into(),
        }
    }

    #[test]
    fn turns_everything_off() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        state::save(&paths, &saved()).unwrap();
        fake.add_process(77, "wayvnc");
        fake.respond("niri msg --json outputs", ok(ON));
        let niri = Niri::new(&fake);

        assert_eq!(
            run(&ctx(&fake, &niri, &paths)).unwrap(),
            "tablet monitor off"
        );
        assert_eq!(
            fake.calls(),
            vec![
                "terminate 77",
                "adb -s ABC123 reverse --remove tcp:5900",
                "niri msg --json outputs",
                "niri msg output Virtual-1 off",
                "notify-send --app-name=tabmon Tablet monitor off The tablet display is disconnected",
            ]
        );
        assert_eq!(state::load(&paths).unwrap(), None);
    }

    #[test]
    fn no_state_means_already_off() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        let niri = Niri::new(&fake);
        assert_eq!(run(&ctx(&fake, &niri, &paths)).unwrap(), "already off");
        assert!(fake.calls().is_empty());
    }

    #[test]
    fn stale_state_is_cleaned_without_killing_anything() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        state::save(&paths, &saved()).unwrap();
        fake.add_process(77, "firefox"); // the PID was reused by another program
        fake.respond(
            "niri msg --json outputs",
            fail("Error: NIRI_SOCKET is not set"),
        );
        let niri = Niri::new(&fake);

        assert_eq!(
            run(&ctx(&fake, &niri, &paths)).unwrap(),
            "already off (cleaned up a stale session)"
        );
        let calls = fake.calls();
        assert!(
            !calls.iter().any(|c| c.starts_with("terminate")),
            "{calls:?}"
        );
        assert!(
            !calls.iter().any(|c| c.starts_with("notify-send")),
            "{calls:?}"
        );
        assert_eq!(state::load(&paths).unwrap(), None);
    }

    #[test]
    fn off_succeeds_when_tablet_was_unplugged() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        state::save(&paths, &saved()).unwrap();
        fake.add_process(77, "wayvnc");
        fake.respond(
            "adb -s ABC123 reverse --remove tcp:5900",
            fail("error: device 'ABC123' not found"),
        );
        fake.respond("niri msg --json outputs", ok(ON));
        let niri = Niri::new(&fake);

        assert_eq!(
            run(&ctx(&fake, &niri, &paths)).unwrap(),
            "tablet monitor off"
        );
        assert!(
            fake.calls()
                .contains(&"niri msg output Virtual-1 off".to_string())
        );
    }

    #[test]
    fn output_already_off_is_left_alone() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        state::save(&paths, &saved()).unwrap();
        fake.add_process(77, "wayvnc");
        fake.respond("niri msg --json outputs", ok(OFF));
        let niri = Niri::new(&fake);

        run(&ctx(&fake, &niri, &paths)).unwrap();
        assert!(
            !fake
                .calls()
                .contains(&"niri msg output Virtual-1 off".to_string())
        );
    }

    #[test]
    fn niri_failure_is_an_error_for_a_live_session() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        state::save(&paths, &saved()).unwrap();
        fake.add_process(77, "wayvnc");
        fake.respond("niri msg --json outputs", ok(ON));
        fake.respond("niri msg output Virtual-1 off", fail("Error: boom"));
        let niri = Niri::new(&fake);

        assert!(run(&ctx(&fake, &niri, &paths)).is_err());
    }
}
