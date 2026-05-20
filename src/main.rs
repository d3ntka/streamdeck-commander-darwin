use anyhow::Result;
use std::{any::{Any, TypeId}, collections::BTreeMap, sync::Arc};
use streamdeck_oxide::{
    button::RenderConfig,
    elgato_streamdeck,
    generic_array::typenum::{U3, U5},
    plugins::{PluginContext, PluginNavigation},
    run_with_external_triggers,
    theme::Theme,
    ExternalTrigger,
};
use tracing::{info};
use tracing_subscriber::{self, EnvFilter};

mod button;
mod config;
mod icons;
mod probe;
mod toggle_command;
mod toggle_icons;
mod toggle_state;

use crate::button::{CommanderContext, CommanderPlugin};
use crate::config::{Config, load_config};
use crate::toggle_state::ToggleStateManager;

#[tokio::main]
async fn main() -> Result<()> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,streamdeck_nix=debug"));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(true)
        .with_line_number(true)
        .init();

    info!("Starting StreamDeck Commander");

    let config: Config = load_config()?;
    let config = Arc::new(config);

    info!("Configuration loaded from embedded config");
    info!("Main menu: {}", config.menu.name);
    info!("Number of buttons: {}", config.menu.buttons.len());

    // Wait for device — normal when moving laptop away from desk
    let hid = elgato_streamdeck::new_hidapi()?;
    let (kind, serial) = {
        let mut logged = false;
        loop {
            let devices = elgato_streamdeck::list_devices(&hid);
            if let Some(device) = devices
                .into_iter()
                .find(|(kind, _)| matches!(kind, elgato_streamdeck::info::Kind::Mk2))
                .or_else(|| elgato_streamdeck::list_devices(&hid).into_iter().next())
            {
                break device;
            }
            if !logged {
                info!("Stream Deck not found, waiting...");
                logged = true;
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    };

    info!("Using Stream Deck: {:?} (Serial: {})", kind, serial);

    let deck = Arc::new(elgato_streamdeck::AsyncStreamDeck::connect(
        &hid, kind, &serial,
    )?);

    info!("Connected to Stream Deck successfully!");

    let render_config = RenderConfig::default();
    let theme = Theme::light();

    let (sender, receiver) = tokio::sync::mpsc::channel::<ExternalTrigger<PluginNavigation<U5, U3>, U5, U3, PluginContext>>(1);

    let toggle_state_manager = ToggleStateManager::new();
    let commander_context = CommanderContext {
        config: config.clone(),
        toggle_state_manager: toggle_state_manager.clone(),
        navigation_sender: Some(sender.clone()),
    };

    let context = PluginContext::new(BTreeMap::from([
        (TypeId::of::<CommanderContext>(), Box::new(Arc::new(commander_context)) as Box<dyn Any + Send + Sync>)
    ]));

    sender.send(ExternalTrigger::new(
        PluginNavigation::<U5, U3>::new(CommanderPlugin::new_with_state_manager(config.menu.clone(), toggle_state_manager)),
        true
    )).await?;

    info!("Starting Stream Deck application...");

    run_with_external_triggers::<PluginNavigation<U5, U3>, U5, U3, PluginContext>(
        theme,
        render_config,
        deck,
        context,
        receiver,
    )
    .await
    .map_err(|e| anyhow::anyhow!("StreamDeck application error: {}", e))?;

    Ok(())
}
