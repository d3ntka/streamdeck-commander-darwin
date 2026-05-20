use anyhow::Result;
use serde::{Deserialize, Serialize};

const EMBEDDED_CONFIG: &str = include_str!("../config.yaml");

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub menu: Menu,
    #[serde(default = "default_brightness")]
    pub brightness: u8,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub idle_sleep_secs: Option<u64>,
}

fn default_brightness() -> u8 { 100 }
fn default_theme() -> String { "dark".to_string() }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Menu {
    pub name: String,
    pub buttons: Vec<Button>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Button {
    Command {
        name: String,
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        icon: Option<String>,
    },
    Menu {
        name: String,
        buttons: Vec<Button>,
        #[serde(default)]
        icon: Option<String>,
    },
    Back {
        #[serde(default = "default_back_name")]
        name: String,
        #[serde(default)]
        icon: Option<String>,
    },
    Toggle {
        name: String,
        #[serde(flatten)]
        mode: ToggleMode,
        #[serde(default)]
        probe_command: Option<String>,
        #[serde(default)]
        probe_args: Vec<String>,
        #[serde(default)]
        on_icon: Option<String>,
        #[serde(default)]
        off_icon: Option<String>,
        #[serde(default)]
        icon: Option<String>,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ToggleMode {
    Single {
        command: String,
        #[serde(default)]
        args: Vec<String>,
    },
    Separate {
        on_command: String,
        #[serde(default)]
        on_args: Vec<String>,
        off_command: String,
        #[serde(default)]
        off_args: Vec<String>,
    },
}

fn default_back_name() -> String {
    "Back".to_string()
}

pub fn load_config() -> Result<Config> {
    tracing::info!("Using embedded configuration");
    let config: Config = serde_yaml::from_str(EMBEDDED_CONFIG)?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config() {
        let yaml = r#"
menu:
  name: "Main Menu"
  buttons:
    - type: command
      name: "List Files"
      command: "ls"
      args: ["-la"]
    - type: menu
      name: "Git Commands"
      buttons:
        - type: command
          name: "Git Status"
          command: "git"
          args: ["status"]
        - type: back
    - type: toggle
      name: "WiFi Toggle"
      mode: single
      command: "nmcli"
      args: ["radio", "wifi"]
      probe_command: "nmcli"
      probe_args: ["radio", "wifi"]
      on_icon: "wifi"
      off_icon: "wifi_off"
    - type: toggle
      name: "VPN Toggle"
      mode: separate
      on_command: "nmcli"
      on_args: ["connection", "up", "vpn"]
      off_command: "nmcli"
      off_args: ["connection", "down", "vpn"]
"#;

        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.menu.name, "Main Menu");
        assert_eq!(config.menu.buttons.len(), 4);

        match &config.menu.buttons[0] {
            Button::Command { name, command, .. } => {
                assert_eq!(name, "List Files");
                assert_eq!(command, "ls");
            }
            _ => panic!("Expected command button"),
        }

        match &config.menu.buttons[1] {
            Button::Menu { name, buttons, .. } => {
                assert_eq!(name, "Git Commands");
                assert_eq!(buttons.len(), 2);
            }
            _ => panic!("Expected menu button"),
        }
    }
}
