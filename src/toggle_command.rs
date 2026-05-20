use crate::config::ToggleMode;
use crate::probe::execute_probe_command;
use crate::toggle_state::{ToggleState, ToggleStateManager};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone)]
pub struct ToggleCommandResult {
    pub success: bool,
    pub new_state: ToggleState,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub error_message: Option<String>,
}

impl ToggleCommandResult {
    pub fn success(new_state: ToggleState, exit_code: i32, stdout: String, stderr: String) -> Self {
        Self {
            success: true,
            new_state,
            exit_code: Some(exit_code),
            stdout,
            stderr,
            error_message: None,
        }
    }

    pub fn failure(
        current_state: ToggleState,
        exit_code: Option<i32>,
        stdout: String,
        stderr: String,
        error_message: String,
    ) -> Self {
        Self {
            success: false,
            new_state: current_state,
            exit_code,
            stdout,
            stderr,
            error_message: Some(error_message),
        }
    }
}

pub async fn execute_toggle_command(
    button_name: &str,
    mode: &ToggleMode,
    probe_command: Option<&str>,
    probe_args: &[String],
    state_manager: &ToggleStateManager,
) -> ToggleCommandResult {
    info!("Executing toggle command for '{}'", button_name);

    let current_state = if let Some(probe_cmd) = probe_command {
        let probe_result = execute_probe_command(probe_cmd, probe_args, button_name).await;
        let probed_state = if probe_result.is_success() {
            ToggleState::On
        } else if probe_result.is_command_failure() {
            ToggleState::Off
        } else {
            ToggleState::Unknown
        };
        state_manager.set_state(button_name, probed_state);
        probed_state
    } else {
        state_manager.get_state(button_name)
    };

    debug!("Current state for '{}': {:?}", button_name, current_state);

    let (command, args, expected_new_state) = match (mode, current_state) {
        (ToggleMode::Single { command, args }, state) => {
            let new_state = match state {
                ToggleState::On => ToggleState::Off,
                ToggleState::Off => ToggleState::On,
                ToggleState::Unknown => ToggleState::On,
            };
            (command.clone(), args.clone(), new_state)
        }
        (ToggleMode::Separate { on_command, on_args, off_command, off_args }, state) => {
            match state {
                ToggleState::On => (off_command.clone(), off_args.clone(), ToggleState::Off),
                ToggleState::Off => (on_command.clone(), on_args.clone(), ToggleState::On),
                ToggleState::Unknown => (on_command.clone(), on_args.clone(), ToggleState::On),
            }
        }
    };

    match execute_command_with_output(&command, &args, button_name).await {
        Ok((exit_code, stdout, stderr)) => {
            if exit_code == 0 {
                state_manager.set_state(button_name, expected_new_state);

                let final_state = if let Some(probe_cmd) = probe_command {
                    let verify_probe = execute_probe_command(probe_cmd, probe_args, button_name).await;
                    let verified_state = if verify_probe.is_success() {
                        ToggleState::On
                    } else if verify_probe.is_command_failure() {
                        ToggleState::Off
                    } else {
                        warn!("Failed to verify new state for '{}', keeping expected state", button_name);
                        expected_new_state
                    };

                    if verified_state != expected_new_state {
                        warn!(
                            "State verification mismatch for '{}': expected {:?}, probed {:?}",
                            button_name, expected_new_state, verified_state
                        );
                    }

                    state_manager.set_state(button_name, verified_state);
                    verified_state
                } else {
                    expected_new_state
                };

                info!("Toggle command for '{}' succeeded, new state: {:?}", button_name, final_state);
                ToggleCommandResult::success(final_state, exit_code, stdout, stderr)
            } else {
                let error_msg = format!("Toggle command failed with exit code {}", exit_code);
                warn!("Toggle command for '{}' failed: {}", button_name, error_msg);
                ToggleCommandResult::failure(current_state, Some(exit_code), stdout, stderr, error_msg)
            }
        }
        Err(e) => {
            let error_msg = format!("Failed to execute toggle command: {}", e);
            error!("Toggle command execution error for '{}': {}", button_name, error_msg);
            ToggleCommandResult::failure(current_state, None, String::new(), String::new(), error_msg)
        }
    }
}

async fn execute_command_with_output(
    command: &str,
    args: &[String],
    button_name: &str,
) -> Result<(i32, String, String), Box<dyn std::error::Error + Send + Sync>> {
    debug!("Executing command for '{}': {} {:?}", button_name, command, args);

    let mut cmd = Command::new(command);
    cmd.args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    match cmd.spawn() {
        Ok(mut child) => {
            let stdout = child.stdout.take().expect("Failed to capture stdout");
            let stderr = child.stderr.take().expect("Failed to capture stderr");

            let stdout_task = tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                let mut output = String::new();
                while let Ok(Some(line)) = lines.next_line().await {
                    if !output.is_empty() { output.push('\n'); }
                    output.push_str(&line);
                }
                output
            });

            let stderr_task = tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                let mut output = String::new();
                while let Ok(Some(line)) = lines.next_line().await {
                    if !output.is_empty() { output.push('\n'); }
                    output.push_str(&line);
                }
                output
            });

            match child.wait().await {
                Ok(status) => {
                    let (stdout_result, stderr_result) = tokio::join!(stdout_task, stderr_task);
                    let stdout = stdout_result.unwrap_or_default();
                    let stderr = stderr_result.unwrap_or_default();
                    Ok((status.code().unwrap_or(-1), stdout, stderr))
                }
                Err(e) => {
                    error!("Failed to wait for command for '{}': {}", button_name, e);
                    Err(Box::new(e))
                }
            }
        }
        Err(e) => {
            error!("Failed to spawn command for '{}': {}", button_name, e);
            Err(Box::new(e))
        }
    }
}
