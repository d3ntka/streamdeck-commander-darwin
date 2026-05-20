pub mod button;
pub mod config;
pub mod icons;
pub mod probe;
pub mod toggle_command;
pub mod toggle_icons;
pub mod toggle_state;

#[cfg(test)]
pub mod toggle_integration_tests;

pub use button::{CommanderContext, CommanderPlugin};
pub use config::{Button, Config, Menu, ToggleMode, load_config};
pub use probe::{ProbeConfig, ProbeResult, execute_probe_command, execute_probe_command_with_config};
pub use toggle_command::{ToggleCommandResult, execute_toggle_command};
pub use toggle_icons::{resolve_toggle_icon, get_toggle_display_name, get_simple_display_name, is_toggle_button, get_toggle_state_description};
pub use toggle_state::{ToggleState, ToggleStateManager};
