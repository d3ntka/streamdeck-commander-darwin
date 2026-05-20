use crate::config::{Button, Config, Menu};
use crate::icons;
use crate::toggle_command::execute_toggle_command;
use crate::toggle_icons::resolve_toggle_icon;
use crate::toggle_state::ToggleStateManager;
use std::{process::Stdio, sync::Arc};
use tokio::io::{AsyncBufReadExt, BufReader};
use streamdeck_oxide::{
    generic_array::typenum::{U3, U5},
    plugins::{Plugin, PluginContext, PluginNavigation},
    ExternalTrigger,
    view::{
        customizable::{ClickButton, CustomizableView},
        View,
    },
};
use tokio::process::Command;
use tracing::{debug, error, info, warn};

#[derive(Clone)]
pub struct CommanderPlugin {
    menu: Menu,
    parent: Option<Box<CommanderPlugin>>,
    toggle_state_manager: ToggleStateManager,
}

pub struct CommanderContext {
    pub config: Arc<Config>,
    pub toggle_state_manager: ToggleStateManager,
    pub navigation_sender: Option<tokio::sync::mpsc::Sender<ExternalTrigger<PluginNavigation<U5, U3>, U5, U3, PluginContext>>>,
}

impl CommanderPlugin {
    pub fn new(menu: Menu) -> Self {
        Self {
            menu,
            parent: None,
            toggle_state_manager: ToggleStateManager::new(),
        }
    }

    pub fn new_with_parent(menu: Menu, parent: CommanderPlugin) -> Self {
        let toggle_state_manager = parent.toggle_state_manager.clone();
        Self {
            menu,
            parent: Some(Box::new(parent)),
            toggle_state_manager,
        }
    }

    pub fn new_with_state_manager(menu: Menu, toggle_state_manager: ToggleStateManager) -> Self {
        Self {
            menu,
            parent: None,
            toggle_state_manager,
        }
    }

    pub fn new_with_parent_and_state_manager(
        menu: Menu,
        parent: Option<Box<CommanderPlugin>>,
        toggle_state_manager: ToggleStateManager,
    ) -> Self {
        Self { menu, parent, toggle_state_manager }
    }

    async fn execute_command(command: &str, args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
        info!("Executing command: {} {:?}", command, args);

        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        match cmd.spawn() {
            Ok(mut child) => {
                let stdout = child.stdout.take().expect("Failed to capture stdout");
                let stderr = child.stderr.take().expect("Failed to capture stderr");

                let stdout_task = {
                    let cmd_str = format!("{} {:?}", command, args);
                    tokio::spawn(async move {
                        let mut lines = BufReader::new(stdout).lines();
                        while let Ok(Some(line)) = lines.next_line().await {
                            debug!("STDOUT [{}]: {}", cmd_str, line);
                        }
                    })
                };

                let stderr_task = {
                    let cmd_str = format!("{} {:?}", command, args);
                    tokio::spawn(async move {
                        let mut lines = BufReader::new(stderr).lines();
                        while let Ok(Some(line)) = lines.next_line().await {
                            debug!("STDERR [{}]: {}", cmd_str, line);
                        }
                    })
                };

                match child.wait().await {
                    Ok(status) => {
                        let _ = tokio::join!(stdout_task, stderr_task);
                        if status.success() {
                            info!("Command executed successfully: {} {:?}", command, args);
                        } else {
                            warn!("Command exited with non-zero status: {} {:?} (exit code: {})",
                                  command, args, status.code().unwrap_or(-1));
                        }
                        Ok(())
                    }
                    Err(e) => {
                        error!("Failed to wait for command: {} {:?} - {}", command, args, e);
                        Err(Box::new(e))
                    }
                }
            }
            Err(e) => {
                error!("Failed to execute command: {} {:?} - {}", command, args, e);
                Err(Box::new(e))
            }
        }
    }

    fn create_view_from_menu(
        &self,
    ) -> Result<Box<dyn View<U5, U3, PluginContext, PluginNavigation<U5, U3>>>, Box<dyn std::error::Error>> {
        let mut view = CustomizableView::new();

        let mut row = 0;
        let mut col = 0;
        let mut button_index = 0;

        for button in &self.menu.buttons {
            // Skip (4,2) — reserved for the back button placed unconditionally below
            if col == 4 && row == 2 {
                col = 0;
                row = 3;
            }

            if row >= 3 {
                break;
            }

            match button {
                Button::Command { name, command, args, icon } => {
                    let command_clone = command.clone();
                    let args_clone = args.clone();
                    let name_clone = name.clone();

                    view.set_button(
                        col,
                        row,
                        ClickButton::new(
                            &name_clone,
                            icons::resolve_icon(icon.as_ref()),
                            move |_context: PluginContext| {
                                let cmd = command_clone.clone();
                                let args = args_clone.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = Self::execute_command(&cmd, &args).await {
                                        error!("Command execution failed: {}", e);
                                    }
                                });
                                async move { Ok(()) }
                            },
                        ),
                    )?;
                }
                Button::Menu { name, buttons, icon } => {
                    let submenu = Menu {
                        name: name.clone(),
                        buttons: buttons.clone(),
                    };

                    view.set_navigation(
                        col,
                        row,
                        PluginNavigation::<U5, U3>::new(CommanderPlugin::new_with_parent(submenu, self.clone())),
                        name,
                        icons::resolve_icon(icon.as_ref()),
                    )?;
                }
                Button::Toggle { name, mode, probe_command, probe_args, .. } => {
                    let button_name = name.clone();
                    let toggle_mode = mode.clone();
                    let probe_cmd = probe_command.clone();
                    let probe_args_clone = probe_args.clone();
                    let state_manager = self.toggle_state_manager.clone();
                    let button_clone = button.clone();
                    let state_manager_for_icon = self.toggle_state_manager.clone();
                    let menu_clone = self.menu.clone();
                    let toggle_state_mgr_clone = self.toggle_state_manager.clone();
                    let parent_for_refresh = self.parent.clone();

                    view.set_button(
                        col,
                        row,
                        ClickButton::new(
                            &button_name.clone(),
                            resolve_toggle_icon(&button_clone, &state_manager_for_icon),
                            move |context: PluginContext| {
                                let name = button_name.clone();
                                let mode = toggle_mode.clone();
                                let probe = probe_cmd.clone();
                                let probe_args = probe_args_clone.clone();
                                let state_mgr = state_manager.clone();
                                let menu_for_refresh = menu_clone.clone();
                                let toggle_state_mgr_for_refresh = toggle_state_mgr_clone.clone();
                                let parent_for_refresh = parent_for_refresh.clone();

                                tokio::spawn(async move {
                                    info!("Toggle button '{}' clicked", name);
                                    let result = execute_toggle_command(
                                        &name,
                                        &mode,
                                        probe.as_deref(),
                                        &probe_args,
                                        &state_mgr,
                                    ).await;

                                    if result.success {
                                        if let Some(commander_ctx) = context.get_context::<CommanderContext>().await {
                                            if let Some(sender) = &commander_ctx.navigation_sender {
                                                let refreshed_plugin = CommanderPlugin::new_with_parent_and_state_manager(menu_for_refresh, parent_for_refresh, toggle_state_mgr_for_refresh);
                                                let refresh_trigger = ExternalTrigger::new(
                                                    PluginNavigation::<U5, U3>::new(refreshed_plugin),
                                                    false
                                                );
                                                if let Err(e) = sender.send(refresh_trigger).await {
                                                    error!("Failed to send refresh trigger: {}", e);
                                                }
                                            }
                                        }
                                    } else {
                                        error!("Toggle '{}' execution failed: {:?}", name, result.error_message);
                                    }
                                });
                                async move { Ok(()) }
                            },
                        ),
                    )?;
                }
                Button::Back { .. } => {
                    debug!("Skipping user-defined back button at position {},{}", col, row);
                    button_index += 1;
                    col += 1;
                    if col >= 5 {
                        col = 0;
                        row += 1;
                    }
                    continue;
                }
            }

            button_index += 1;
            col += 1;
            if col >= 5 {
                col = 0;
                row += 1;
            }
        }

        if self.parent.is_some() {
            if let Some(parent) = &self.parent {
                view.set_navigation(
                    4,
                    2,
                    PluginNavigation::<U5, U3>::new(parent.as_ref().clone()),
                    "Back",
                    icons::resolve_icon(Some(&"arrow_back".to_string())),
                )?;
            }
        }

        Ok(Box::new(view))
    }

    async fn probe_initial_toggle_states(&self, context: &PluginContext) {
        let mut needs_refresh = false;

        for button in &self.menu.buttons {
            if let Button::Toggle { name, probe_command, probe_args, .. } = button {
                if let Some(probe_cmd) = probe_command {
                    let probe_result = crate::probe::execute_probe_command(
                        probe_cmd,
                        probe_args,
                        name,
                    ).await;

                    let initial_state = if probe_result.is_success() {
                        crate::toggle_state::ToggleState::On
                    } else {
                        crate::toggle_state::ToggleState::Off
                    };

                    let old_state = self.toggle_state_manager.get_state(name);
                    if matches!(old_state, crate::toggle_state::ToggleState::Unknown) {
                        self.toggle_state_manager.set_state(name, initial_state);
                        needs_refresh = true;
                    }
                }
            }
        }

        if needs_refresh {
            if let Some(commander_ctx) = context.get_context::<CommanderContext>().await {
                if let Some(sender) = &commander_ctx.navigation_sender {
                    let refreshed_plugin = CommanderPlugin::new_with_parent_and_state_manager(
                        self.menu.clone(),
                        self.parent.clone(),
                        self.toggle_state_manager.clone(),
                    );
                    let refresh_trigger = ExternalTrigger::new(
                        PluginNavigation::<U5, U3>::new(refreshed_plugin),
                        false
                    );
                    if let Err(e) = sender.send(refresh_trigger).await {
                        error!("Failed to send initial state refresh trigger: {}", e);
                    }
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl Plugin<U5, U3> for CommanderPlugin {
    fn name(&self) -> &'static str {
        "StreamDeck Commander"
    }

    async fn get_view(&self, context: PluginContext) -> Result<Box<dyn View<U5, U3, PluginContext, PluginNavigation<U5, U3>>>, Box<dyn std::error::Error>> {
        info!("Creating view for menu: {}", self.menu.name);
        self.probe_initial_toggle_states(&context).await;
        self.create_view_from_menu()
    }
}
