use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{debug, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToggleState {
    On,
    Off,
    Unknown,
}

impl ToggleState {
    pub fn toggle(self) -> ToggleState {
        match self {
            ToggleState::On => ToggleState::Off,
            ToggleState::Off => ToggleState::On,
            ToggleState::Unknown => ToggleState::Unknown,
        }
    }

    pub fn is_known(self) -> bool {
        matches!(self, ToggleState::On | ToggleState::Off)
    }
}

#[derive(Debug)]
pub struct ToggleStateManager {
    states: Arc<RwLock<HashMap<String, ToggleState>>>,
}

impl Clone for ToggleStateManager {
    fn clone(&self) -> Self {
        Self {
            states: Arc::clone(&self.states),
        }
    }
}

impl Default for ToggleStateManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ToggleStateManager {
    pub fn new() -> Self {
        Self {
            states: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn get_state(&self, button_name: &str) -> ToggleState {
        match self.states.read() {
            Ok(states) => {
                let state = states.get(button_name).copied().unwrap_or(ToggleState::Unknown);
                debug!("Retrieved state for '{}': {:?}", button_name, state);
                state
            }
            Err(e) => {
                warn!("Failed to read toggle state for '{}': {}", button_name, e);
                ToggleState::Unknown
            }
        }
    }

    pub fn set_state(&self, button_name: &str, state: ToggleState) {
        match self.states.write() {
            Ok(mut states) => {
                let previous = states.insert(button_name.to_string(), state);
                debug!(
                    "Set state for '{}': {:?} -> {:?}",
                    button_name, previous.unwrap_or(ToggleState::Unknown), state
                );
            }
            Err(e) => {
                warn!("Failed to set toggle state for '{}': {}", button_name, e);
            }
        }
    }

    pub fn toggle_state(&self, button_name: &str) -> ToggleState {
        let current_state = self.get_state(button_name);
        let new_state = current_state.toggle();
        self.set_state(button_name, new_state);
        new_state
    }

    pub fn update_from_probe(&self, button_name: &str, probe_success: bool) {
        let new_state = if probe_success {
            ToggleState::On
        } else {
            ToggleState::Off
        };
        self.set_state(button_name, new_state);
    }

    pub fn clear_all(&self) {
        match self.states.write() {
            Ok(mut states) => {
                let count = states.len();
                states.clear();
                debug!("Cleared {} toggle states", count);
            }
            Err(e) => {
                warn!("Failed to clear toggle states: {}", e);
            }
        }
    }

    pub fn get_all_states(&self) -> HashMap<String, ToggleState> {
        match self.states.read() {
            Ok(states) => states.clone(),
            Err(e) => {
                warn!("Failed to read all toggle states: {}", e);
                HashMap::new()
            }
        }
    }

    pub fn button_count(&self) -> usize {
        match self.states.read() {
            Ok(states) => states.len(),
            Err(_) => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toggle_state_toggle() {
        assert_eq!(ToggleState::On.toggle(), ToggleState::Off);
        assert_eq!(ToggleState::Off.toggle(), ToggleState::On);
        assert_eq!(ToggleState::Unknown.toggle(), ToggleState::Unknown);
    }

    #[test]
    fn test_toggle_state_is_known() {
        assert!(ToggleState::On.is_known());
        assert!(ToggleState::Off.is_known());
        assert!(!ToggleState::Unknown.is_known());
    }

    #[test]
    fn test_toggle_state_manager_basic() {
        let manager = ToggleStateManager::new();

        assert_eq!(manager.get_state("test"), ToggleState::Unknown);

        manager.set_state("test", ToggleState::On);
        assert_eq!(manager.get_state("test"), ToggleState::On);

        let new_state = manager.toggle_state("test");
        assert_eq!(new_state, ToggleState::Off);
        assert_eq!(manager.get_state("test"), ToggleState::Off);
    }

    #[test]
    fn test_toggle_state_manager_clone() {
        let manager1 = ToggleStateManager::new();
        manager1.set_state("test", ToggleState::On);

        let manager2 = manager1.clone();
        assert_eq!(manager2.get_state("test"), ToggleState::On);

        manager2.set_state("test", ToggleState::Off);
        assert_eq!(manager1.get_state("test"), ToggleState::Off);
    }
}
