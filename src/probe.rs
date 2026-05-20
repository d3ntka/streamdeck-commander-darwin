use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl ProbeResult {
    pub fn success(exit_code: i32, stdout: String, stderr: String) -> Self {
        Self { success: true, exit_code: Some(exit_code), stdout, stderr }
    }

    pub fn failure(exit_code: Option<i32>, stdout: String, stderr: String) -> Self {
        Self { success: false, exit_code, stdout, stderr }
    }

    pub fn execution_error(error_message: String) -> Self {
        Self { success: false, exit_code: None, stdout: String::new(), stderr: error_message }
    }

    pub fn is_success(&self) -> bool {
        self.success && self.exit_code == Some(0)
    }

    pub fn is_command_failure(&self) -> bool {
        !self.success && self.exit_code.is_some()
    }

    pub fn is_execution_error(&self) -> bool {
        !self.success && self.exit_code.is_none()
    }
}

pub async fn execute_probe_command(
    command: &str,
    args: &[String],
    button_name: &str,
) -> ProbeResult {
    info!("Executing probe command for '{}': {} {:?}", button_name, command, args);

    let mut cmd = Command::new(command);
    cmd.args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());

    match cmd.output().await {
        Ok(output) => {
            let exit_code = output.status.code();
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let success = output.status.success();

            debug!(
                "Probe command for '{}' completed: exit_code={:?}, success={}",
                button_name, exit_code, success
            );

            if success {
                ProbeResult::success(exit_code.unwrap_or(0), stdout, stderr)
            } else {
                ProbeResult::failure(exit_code, stdout, stderr)
            }
        }
        Err(e) => {
            error!("Failed to execute probe command for '{}': {} {:?} - {}", button_name, command, args, e);
            ProbeResult::execution_error(format!("Command execution failed: {}", e))
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProbeConfig {
    pub timeout_ms: u64,
    pub empty_stdout_is_success: bool,
    pub success_indicators: Vec<String>,
    pub failure_indicators: Vec<String>,
}

impl Default for ProbeConfig {
    fn default() -> Self {
        Self {
            timeout_ms: 5000,
            empty_stdout_is_success: true,
            success_indicators: Vec::new(),
            failure_indicators: Vec::new(),
        }
    }
}

pub async fn execute_probe_command_with_config(
    command: &str,
    args: &[String],
    button_name: &str,
    config: &ProbeConfig,
) -> ProbeResult {
    info!(
        "Executing probe command with config for '{}': {} {:?} (timeout: {}ms)",
        button_name, command, args, config.timeout_ms
    );

    let mut cmd = Command::new(command);
    cmd.args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());

    let timeout_duration = std::time::Duration::from_millis(config.timeout_ms);

    match tokio::time::timeout(timeout_duration, cmd.output()).await {
        Ok(Ok(output)) => {
            let exit_code = output.status.code();
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let exit_success = output.status.success();

            let custom_success = evaluate_custom_indicators(&stdout, config);
            let final_success = custom_success.unwrap_or(exit_success);

            if final_success {
                ProbeResult::success(exit_code.unwrap_or(0), stdout, stderr)
            } else {
                ProbeResult::failure(exit_code, stdout, stderr)
            }
        }
        Ok(Err(e)) => {
            error!("Failed to execute probe command for '{}': {}", button_name, e);
            ProbeResult::execution_error(format!("Command execution failed: {}", e))
        }
        Err(_) => {
            warn!("Probe command for '{}' timed out after {}ms", button_name, config.timeout_ms);
            ProbeResult::execution_error(format!("Command timed out after {}ms", config.timeout_ms))
        }
    }
}

fn evaluate_custom_indicators(stdout: &str, config: &ProbeConfig) -> Option<bool> {
    for indicator in &config.failure_indicators {
        if stdout.contains(indicator) {
            return Some(false);
        }
    }
    for indicator in &config.success_indicators {
        if stdout.contains(indicator) {
            return Some(true);
        }
    }
    if stdout.trim().is_empty() {
        return Some(config.empty_stdout_is_success);
    }
    None
}
