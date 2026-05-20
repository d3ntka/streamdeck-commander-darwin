//! Integration tests for toggle button functionality

use crate::config::{Button, Menu, ToggleMode};
use crate::probe::{execute_probe_command, ProbeConfig, execute_probe_command_with_config};
use crate::toggle_command::execute_toggle_command;
use crate::toggle_icons::{resolve_toggle_icon, get_toggle_display_name, is_toggle_button};
use crate::toggle_state::{ToggleState, ToggleStateManager};

#[cfg(test)]
mod tests {
    use super::*;

    fn create_single_mode_toggle() -> Button {
        Button::Toggle {
            name: "WiFi".to_string(),
            mode: ToggleMode::Single {
                command: "nmcli".to_string(),
                args: vec!["radio".to_string(), "wifi".to_string()],
            },
            probe_command: Some("nmcli".to_string()),
            probe_args: vec!["radio".to_string(), "wifi".to_string()],
            on_icon: Some("wifi".to_string()),
            off_icon: Some("wifi_off".to_string()),
            icon: Some("settings".to_string()),
        }
    }

    fn create_separate_mode_toggle() -> Button {
        Button::Toggle {
            name: "VPN".to_string(),
            mode: ToggleMode::Separate {
                on_command: "systemctl".to_string(),
                on_args: vec!["start".to_string(), "openvpn".to_string()],
                off_command: "systemctl".to_string(),
                off_args: vec!["stop".to_string(), "openvpn".to_string()],
            },
            probe_command: Some("systemctl".to_string()),
            probe_args: vec!["is-active".to_string(), "openvpn".to_string()],
            on_icon: Some("vpn_key".to_string()),
            off_icon: Some("vpn_key_off".to_string()),
            icon: None,
        }
    }

    fn create_test_menu() -> Menu {
        Menu {
            name: "Test Menu".to_string(),
            buttons: vec![
                Button::Command {
                    name: "Test Command".to_string(),
                    command: "echo".to_string(),
                    args: vec!["hello".to_string()],
                    icon: Some("terminal".to_string()),
                },
                create_single_mode_toggle(),
                create_separate_mode_toggle(),
                Button::Menu {
                    name: "Submenu".to_string(),
                    buttons: vec![create_single_mode_toggle()],
                    icon: Some("folder".to_string()),
                },
            ],
        }
    }

    #[test]
    fn test_toggle_button_identification() {
        assert!(is_toggle_button(&create_single_mode_toggle()));
        assert!(is_toggle_button(&create_separate_mode_toggle()));
        assert!(!is_toggle_button(&Button::Command {
            name: "Test".to_string(),
            command: "echo".to_string(),
            args: vec![],
            icon: None,
        }));
    }

    #[test]
    fn test_toggle_state_management_integration() {
        let state_manager = ToggleStateManager::new();

        assert_eq!(state_manager.get_state("WiFi"), ToggleState::Unknown);

        state_manager.set_state("WiFi", ToggleState::Off);
        assert_eq!(state_manager.get_state("WiFi"), ToggleState::Off);

        let new_state = state_manager.toggle_state("WiFi");
        assert_eq!(new_state, ToggleState::On);

        state_manager.update_from_probe("WiFi", false);
        assert_eq!(state_manager.get_state("WiFi"), ToggleState::Off);
    }

    #[test]
    fn test_toggle_display_names() {
        let state_manager = ToggleStateManager::new();
        let button = create_single_mode_toggle();

        state_manager.set_state("WiFi", ToggleState::On);
        assert_eq!(get_toggle_display_name(&button, &state_manager), "WiFi ●");

        state_manager.set_state("WiFi", ToggleState::Off);
        assert_eq!(get_toggle_display_name(&button, &state_manager), "WiFi ○");

        state_manager.set_state("WiFi", ToggleState::Unknown);
        assert_eq!(get_toggle_display_name(&button, &state_manager), "WiFi ?");
    }

    #[tokio::test]
    async fn test_probe_command_execution() {
        let result = execute_probe_command("true", &[], "test-probe").await;
        assert!(result.is_success());

        let result = execute_probe_command("false", &[], "test-probe").await;
        assert!(!result.is_success());
        assert!(result.is_command_failure());

        let result = execute_probe_command("nonexistent_command_xyz", &[], "test-probe").await;
        assert!(result.is_execution_error());
    }

    #[tokio::test]
    async fn test_probe_with_config() {
        let config = ProbeConfig {
            timeout_ms: 1000,
            empty_stdout_is_success: true,
            success_indicators: vec!["active".to_string()],
            failure_indicators: vec!["inactive".to_string()],
        };

        let result = execute_probe_command_with_config(
            "echo",
            &["Service is active".to_string()],
            "test-probe",
            &config
        ).await;
        assert!(result.is_success());

        let result = execute_probe_command_with_config(
            "echo",
            &["Service is inactive".to_string()],
            "test-probe",
            &config
        ).await;
        assert!(!result.is_success());
    }

    #[tokio::test]
    async fn test_single_mode_toggle_execution() {
        let state_manager = ToggleStateManager::new();
        let mode = ToggleMode::Single {
            command: "echo".to_string(),
            args: vec!["toggling".to_string()],
        };

        let result = execute_toggle_command("test", &mode, None, &[], &state_manager).await;
        assert!(result.success);
        assert_eq!(result.new_state, ToggleState::On);

        state_manager.set_state("test", ToggleState::On);
        let result = execute_toggle_command("test", &mode, None, &[], &state_manager).await;
        assert!(result.success);
        assert_eq!(result.new_state, ToggleState::Off);
    }

    #[tokio::test]
    async fn test_separate_mode_toggle_execution() {
        let state_manager = ToggleStateManager::new();
        let mode = ToggleMode::Separate {
            on_command: "echo".to_string(),
            on_args: vec!["turning_on".to_string()],
            off_command: "echo".to_string(),
            off_args: vec!["turning_off".to_string()],
        };

        state_manager.set_state("test", ToggleState::Off);
        let result = execute_toggle_command("test", &mode, None, &[], &state_manager).await;
        assert!(result.success);
        assert_eq!(result.new_state, ToggleState::On);
        assert!(result.stdout.contains("turning_on"));

        state_manager.set_state("test", ToggleState::On);
        let result = execute_toggle_command("test", &mode, None, &[], &state_manager).await;
        assert!(result.success);
        assert_eq!(result.new_state, ToggleState::Off);
        assert!(result.stdout.contains("turning_off"));
    }

    #[tokio::test]
    async fn test_toggle_command_failure_handling() {
        let state_manager = ToggleStateManager::new();
        let mode = ToggleMode::Single {
            command: "false".to_string(),
            args: vec![],
        };

        state_manager.set_state("test", ToggleState::Off);
        let result = execute_toggle_command("test", &mode, None, &[], &state_manager).await;

        assert!(!result.success);
        assert_eq!(result.new_state, ToggleState::Off);
        assert!(result.error_message.is_some());
    }

    #[test]
    fn test_menu_with_toggles_creation() {
        let menu = create_test_menu();
        assert_eq!(menu.buttons.len(), 4);

        let toggle_count = menu.buttons.iter()
            .filter(|b| is_toggle_button(b))
            .count();
        assert_eq!(toggle_count, 2);
    }

    #[test]
    fn test_state_manager_thread_safety() {
        use std::sync::Arc;
        use std::thread;

        let state_manager = Arc::new(ToggleStateManager::new());
        let mut handles = vec![];

        for i in 0..10 {
            let manager = Arc::clone(&state_manager);
            let handle = thread::spawn(move || {
                let button_name = format!("button_{}", i);
                manager.set_state(&button_name, ToggleState::On);
                manager.toggle_state(&button_name);
                manager.get_state(&button_name)
            });
            handles.push(handle);
        }

        for handle in handles {
            let final_state = handle.join().unwrap();
            assert_eq!(final_state, ToggleState::Off);
        }

        assert_eq!(state_manager.button_count(), 10);
    }
}
