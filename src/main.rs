use anyhow::Result;
use std::{
    any::{Any, TypeId},
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use streamdeck_oxide::{
    button::RenderConfig,
    elgato_streamdeck,
    generic_array::typenum::{U3, U5},
    plugins::{PluginContext, PluginNavigation},
    run_with_external_triggers,
    theme::Theme,
    ExternalTrigger,
};
use tokio::time::Duration;
use tracing::info;
use tracing_subscriber::{self, EnvFilter};

mod button;
mod config;
mod decorations;
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

    info!("Main menu: {}", config.menu.name);

    let hid = elgato_streamdeck::new_hidapi()?;
    let (kind, serial) = {
        let mut logged = false;
        loop {
            let devices = elgato_streamdeck::list_devices(&hid);
            if let Some(device) = devices
                .iter()
                .find(|(kind, _)| matches!(kind, elgato_streamdeck::info::Kind::Mk2))
                .or_else(|| devices.first())
                .cloned()
            {
                break device;
            }
            if !logged {
                info!("Stream Deck not found, waiting...");
                logged = true;
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    };

    info!("Using Stream Deck: {:?} (Serial: {})", kind, serial);

    let deck = Arc::new(elgato_streamdeck::AsyncStreamDeck::connect(&hid, kind, &serial)?);

    info!("Connected to Stream Deck successfully!");

    deck.set_brightness(config.brightness).await?;

    let render_config = RenderConfig::default();
    let theme = match config.theme.as_str() {
        "light" => Theme::light(),
        _ => Theme::dark(),
    };

    let (nav_sender, receiver) =
        tokio::sync::mpsc::channel::<ExternalTrigger<PluginNavigation<U5, U3>, U5, U3, PluginContext>>(1);
    let (activity_sender, mut activity_receiver) = tokio::sync::mpsc::channel::<()>(16);
    let sleeping = Arc::new(AtomicBool::new(false));

    let toggle_state_manager = ToggleStateManager::new();
    let commander_context = CommanderContext {
        config: config.clone(),
        toggle_state_manager: toggle_state_manager.clone(),
        navigation_sender: Some(nav_sender.clone()),
        activity_sender: Some(activity_sender),
        sleeping: Some(sleeping.clone()),
    };

    let context = PluginContext::new(BTreeMap::from([(
        TypeId::of::<CommanderContext>(),
        Box::new(Arc::new(commander_context)) as Box<dyn Any + Send + Sync>,
    )]));

    nav_sender.send(ExternalTrigger::new(
        PluginNavigation::<U5, U3>::new(CommanderPlugin::new_with_state_manager(
            config.menu.clone(),
            toggle_state_manager,
        )),
        true,
    )).await?;

    // Idle sleep task
    if let Some(idle_secs) = config.idle_sleep_secs {
        let deck_idle = deck.clone();
        let brightness = config.brightness;
        let sleeping_task = sleeping.clone();
        tokio::spawn(async move {
            let mut last_activity = std::time::Instant::now();
            loop {
                tokio::select! {
                    msg = activity_receiver.recv() => {
                        if msg.is_none() { break; }
                        last_activity = std::time::Instant::now();
                        if sleeping_task.load(Ordering::SeqCst) {
                            sleeping_task.store(false, Ordering::SeqCst);
                            deck_idle.set_brightness(brightness).await.ok();
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_secs(10)) => {
                        if !sleeping_task.load(Ordering::SeqCst)
                            && last_activity.elapsed().as_secs() >= idle_secs
                        {
                            info!("Idle {}s, dimming Stream Deck", idle_secs);
                            sleeping_task.store(true, Ordering::SeqCst);
                            deck_idle.set_brightness(0).await.ok();
                        }
                    }
                }
            }
        });
    }

    info!("Starting Stream Deck application...");

    run_with_external_triggers::<PluginNavigation<U5, U3>, U5, U3, PluginContext>(
        theme,
        render_config,
        deck,
        context,
        receiver,
    )
    .await
    .map_err(|e| anyhow::anyhow!("StreamDeck error: {}", e))?;

    Ok(())
}
