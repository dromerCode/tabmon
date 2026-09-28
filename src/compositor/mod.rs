pub mod niri;

use anyhow::Result;

use crate::error::UserError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputState {
    Missing,
    Off,
    On,
}

/// What tabmon needs from a Wayland compositor.
pub trait Compositor {
    fn output_state(&self, output: &str) -> Result<OutputState>;
    fn set_output(&self, output: &str, on: bool) -> Result<()>;
}

pub fn missing_output(output: &str) -> UserError {
    UserError::new(format!("output {output} not found in niri")).hint(format!(
        "load the vkms module (sudo modprobe vkms) and add an `output \"{output}\"` block to niri's config.kdl; see the README"
    ))
}
