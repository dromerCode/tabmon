use std::fmt;

/// An error meant for the user: what went wrong and, when known, how to fix it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserError {
    pub message: String,
    pub hint: Option<String>,
}

impl UserError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            hint: None,
        }
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

impl fmt::Display for UserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for UserError {}

/// Formats an error for stderr: `error: ...` plus `hint: ...` when there is one.
pub fn render(err: &anyhow::Error) -> String {
    match err.downcast_ref::<UserError>() {
        Some(UserError {
            message,
            hint: Some(hint),
        }) => format!("error: {message}\nhint: {hint}"),
        Some(UserError {
            message,
            hint: None,
        }) => format!("error: {message}"),
        None => format!("error: {err:#}"),
    }
}

/// Appends the failures of a rollback to the error that caused it, keeping its hint.
pub fn with_rollback_failures(
    original: anyhow::Error,
    failures: Vec<anyhow::Error>,
) -> anyhow::Error {
    if failures.is_empty() {
        return original;
    }
    let extra = failures
        .iter()
        .map(|e| format!("{e:#}"))
        .collect::<Vec<_>>()
        .join("; ");
    let suffix = format!(" (undoing the previous steps also failed: {extra})");
    match original.downcast::<UserError>() {
        Ok(mut e) => {
            e.message.push_str(&suffix);
            e.into()
        }
        Err(original) => anyhow::anyhow!("{original:#}{suffix}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;

    #[test]
    fn renders_message_and_hint() {
        let err: anyhow::Error = UserError::new("no adb device found")
            .hint("connect the tablet by USB")
            .into();
        assert_eq!(
            render(&err),
            "error: no adb device found\nhint: connect the tablet by USB"
        );
    }

    #[test]
    fn renders_message_without_hint() {
        let err: anyhow::Error = UserError::new("something broke").into();
        assert_eq!(render(&err), "error: something broke");
    }

    #[test]
    fn renders_plain_errors_with_their_context_chain() {
        let err = Err::<(), _>(std::io::Error::other("disk full"))
            .context("could not write state")
            .unwrap_err();
        assert_eq!(render(&err), "error: could not write state: disk full");
    }

    #[test]
    fn rollback_failures_are_appended_and_hint_is_kept() {
        let original: anyhow::Error = UserError::new("adb reverse failed")
            .hint("reconnect the tablet")
            .into();
        let err = with_rollback_failures(original, vec![anyhow::anyhow!("niri refused")]);
        assert_eq!(
            render(&err),
            "error: adb reverse failed (undoing the previous steps also failed: niri refused)\nhint: reconnect the tablet"
        );
    }

    #[test]
    fn no_rollback_failures_leaves_error_untouched() {
        let original: anyhow::Error = UserError::new("adb reverse failed").into();
        let err = with_rollback_failures(original, vec![]);
        assert_eq!(render(&err), "error: adb reverse failed");
    }
}
