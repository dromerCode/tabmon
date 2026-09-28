use anyhow::Result;

use super::{Ctx, active_state, off, on};

pub fn run(ctx: &Ctx) -> Result<String> {
    if active_state(ctx)?.is_some() {
        off::run(ctx)
    } else {
        on::run(ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::tests_support::{ADB_ONE, OFF, ON, ctx};
    use crate::compositor::niri::Niri;
    use crate::state::{self, State};
    use crate::testing::{FakeSystem, ok, temp_paths};

    #[test]
    fn toggles_off_when_active() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        state::save(
            &paths,
            &State {
                pid: 77,
                port: 5900,
                serial: "ABC123".into(),
                output: "Virtual-1".into(),
                created_reverse: true,
            },
        )
        .unwrap();
        fake.add_process(77, "wayvnc");
        fake.respond("niri msg --json outputs", ok(ON));
        let niri = Niri::new(&fake);

        assert_eq!(
            run(&ctx(&fake, &niri, &paths)).unwrap(),
            "tablet monitor off"
        );
    }

    #[test]
    fn toggles_on_when_inactive() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        fake.respond("niri msg --json outputs", ok(OFF));
        fake.respond("niri msg --json outputs", ok(ON));
        fake.respond("adb devices", ok(ADB_ONE));
        let niri = Niri::new(&fake);

        assert!(
            run(&ctx(&fake, &niri, &paths))
                .unwrap()
                .starts_with("tablet monitor on")
        );
    }
}
