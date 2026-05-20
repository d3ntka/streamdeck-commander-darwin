use crate::config::Button;
use crate::icons::resolve_icon;
use crate::toggle_state::{ToggleState, ToggleStateManager};
use tracing::debug;

pub fn resolve_toggle_icon(
    button: &Button,
    state_manager: &ToggleStateManager,
) -> Option<&'static str> {
    match button {
        Button::Toggle { name, on_icon, off_icon, icon, .. } => {
            let current_state = state_manager.get_state(name);
            debug!("Resolving icon for toggle '{}' in state {:?}", name, current_state);

            match current_state {
                ToggleState::On => {
                    on_icon.as_ref().and_then(|i| resolve_icon(Some(i)))
                        .or_else(|| icon.as_ref().and_then(|i| resolve_icon(Some(i))))
                        .or_else(|| resolve_icon(Some(&"toggle_on".to_string())))
                }
                ToggleState::Off => {
                    off_icon.as_ref().and_then(|i| resolve_icon(Some(i)))
                        .or_else(|| icon.as_ref().and_then(|i| resolve_icon(Some(i))))
                        .or_else(|| resolve_icon(Some(&"toggle_off".to_string())))
                }
                ToggleState::Unknown => {
                    icon.as_ref().and_then(|i| resolve_icon(Some(i)))
                        .or_else(|| resolve_icon(Some(&"help".to_string())))
                }
            }
        }
        Button::Command { icon, .. }
        | Button::Menu { icon, .. }
        | Button::Back { icon, .. } => resolve_icon(icon.as_ref()),
    }
}

pub fn get_toggle_display_name(button: &Button, state_manager: &ToggleStateManager) -> String {
    match button {
        Button::Toggle { name, .. } => {
            let current_state = state_manager.get_state(name);
            match current_state {
                ToggleState::On => format!("{} ●", name),
                ToggleState::Off => format!("{} ○", name),
                ToggleState::Unknown => format!("{} ?", name),
            }
        }
        Button::Command { name, .. }
        | Button::Menu { name, .. }
        | Button::Back { name, .. } => name.clone(),
    }
}

pub fn get_simple_display_name(button: &Button) -> &str {
    match button {
        Button::Command { name, .. }
        | Button::Menu { name, .. }
        | Button::Back { name, .. }
        | Button::Toggle { name, .. } => name,
    }
}

pub fn is_toggle_button(button: &Button) -> bool {
    matches!(button, Button::Toggle { .. })
}

pub fn get_toggle_state_description(button: &Button, state_manager: &ToggleStateManager) -> Option<String> {
    match button {
        Button::Toggle { name, .. } => {
            let state = state_manager.get_state(name);
            Some(match state {
                ToggleState::On => "Currently enabled".to_string(),
                ToggleState::Off => "Currently disabled".to_string(),
                ToggleState::Unknown => "State unknown".to_string(),
            })
        }
        _ => None,
    }
}
