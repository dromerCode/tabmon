use crate::error::render;
use crate::system::System;

/// Desktop notification; failures are ignored because notifications are optional.
pub fn send(sys: &dyn System, summary: &str, body: &str, urgent: bool) {
    let mut args = vec!["--app-name=tabmon"];
    if urgent {
        args.push("--urgency=critical");
    }
    args.extend([summary, body]);
    let _ = sys.run("notify-send", &args);
}

/// Text to print for a failed command; also shown as a notification, since the
/// keybinding has no terminal to print to.
pub fn failure(sys: &dyn System, err: &anyhow::Error, notify: bool) -> String {
    let text = render(err);
    if notify {
        send(sys, "tabmon failed", &text, true);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::UserError;
    use crate::testing::FakeSystem;

    #[test]
    fn send_calls_notify_send() {
        let fake = FakeSystem::new();
        send(
            &fake,
            "Tablet monitor on",
            "Connect to localhost:5900",
            false,
        );
        assert_eq!(
            fake.calls(),
            vec!["notify-send --app-name=tabmon Tablet monitor on Connect to localhost:5900"]
        );
    }

    #[test]
    fn send_ignores_missing_notify_send() {
        let fake = FakeSystem::new();
        fake.respond_err(
            "notify-send --app-name=tabmon a b",
            UserError::new("notify-send not found"),
        );
        send(&fake, "a", "b", false);
    }

    #[test]
    fn failure_is_rendered_and_notified_as_urgent() {
        let fake = FakeSystem::new();
        let err: anyhow::Error = UserError::new("no adb device found")
            .hint("plug it in")
            .into();
        let text = failure(&fake, &err, true);
        assert_eq!(text, "error: no adb device found\nhint: plug it in");
        assert_eq!(
            fake.calls(),
            vec![
                "notify-send --app-name=tabmon --urgency=critical tabmon failed error: no adb device found\nhint: plug it in"
            ]
        );
    }

    #[test]
    fn failure_without_notify_only_renders() {
        let fake = FakeSystem::new();
        let err: anyhow::Error = UserError::new("boom").into();
        assert_eq!(failure(&fake, &err, false), "error: boom");
        assert!(fake.calls().is_empty());
    }
}
