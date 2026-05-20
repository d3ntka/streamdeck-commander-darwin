use anyhow::Result;
use std::{
    any::{Any, TypeId},
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use streamdeck_oxide::{
    button::RenderConfig,
    elgato_streamdeck,
    generic_array::typenum::{U3, U5},
    plugins::{PluginContext, PluginNavigation},
    DisplayManager,
    theme::Theme,
    ExternalTrigger,
};
use tokio::time::Duration;
use tracing::info;
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

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

type Nav = PluginNavigation<U5, U3>;
type Trigger = ExternalTrigger<Nav, U5, U3, PluginContext>;

async fn run(
    config: Arc<Config>,
    deck: Arc<elgato_streamdeck::AsyncStreamDeck>,
    render_config: RenderConfig,
    theme: Theme,
    context: PluginContext,
    mut receiver: tokio::sync::mpsc::Receiver<Trigger>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (display_manager, mut nav_receiver) =
        DisplayManager::<Nav, U5, U3, PluginContext>::new(deck.clone(), render_config, theme, context).await?;

    display_manager.fetch_all().await?;
    display_manager.render().await?;

    let reader = deck.get_reader();
    let last_activity = Arc::new(AtomicU64::new(now_secs()));
    let is_sleeping = Arc::new(AtomicBool::new(false));

    let mut idle_tick = tokio::time::interval(Duration::from_secs(30));
    idle_tick.tick().await; // skip the immediate first tick

    loop {
        tokio::select! {
            events = reader.read(10.0) => {
                for event in events? {
                    match event {
                        elgato_streamdeck::DeviceStateUpdate::ButtonDown(id) => {
                            last_activity.store(now_secs(), Ordering::Relaxed);
                            if is_sleeping.swap(false, Ordering::Relaxed) {
                                deck.set_brightness(config.brightness).await?;
                                display_manager.fetch_all().await?;
                                display_manager.render().await?;
                            } else {
                                display_manager.on_press(id).await?;
                            }
                        }
                        elgato_streamdeck::DeviceStateUpdate::ButtonUp(id) => {
                            last_activity.store(now_secs(), Ordering::Relaxed);
                            if !is_sleeping.load(Ordering::Relaxed) {
                                display_manager.on_release(id).await?;
                            }
                        }
                        _ => {}
                    }
                }
            }
            Some(nav) = nav_receiver.recv() => {
                display_manager.navigate_to(nav).await?;
                display_manager.fetch_all().await?;
                display_manager.render().await?;
            }
            Some(trigger) = receiver.recv() => {
                if trigger.switch_view || trigger.navigation == display_manager.get_current_navigation().await? {
                    display_manager.navigate_to(trigger.navigation).await?;
                    display_manager.fetch_all().await?;
                    display_manager.render().await?;
                }
            }
            _ = idle_tick.tick() => {
                if let Some(idle_secs) = config.idle_sleep_secs {
                    let idle = now_secs().saturating_sub(last_activity.load(Ordering::Relaxed));
                    if !is_sleeping.load(Ordering::Relaxed) && idle >= idle_secs {
                        info!("Idle {}s, dimming Stream Deck", idle_secs);
                        deck.set_brightness(0).await?;
                        is_sleeping.store(true, Ordering::Relaxed);
                    }
                }
            }
        }
    }
}

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

    let (sender, receiver) = tokio::sync::mpsc::channel::<Trigger>(1);

    let toggle_state_manager = ToggleStateManager::new();
    let commander_context = CommanderContext {
        config: config.clone(),
        toggle_state_manager: toggle_state_manager.clone(),
        navigation_sender: Some(sender.clone()),
    };

    let context = PluginContext::new(BTreeMap::from([(
        TypeId::of::<CommanderContext>(),
        Box::new(Arc::new(commander_context)) as Box<dyn Any + Send + Sync>,
    )]));

    sender.send(ExternalTrigger::new(
        Nav::new(CommanderPlugin::new_with_state_manager(config.menu.clone(), toggle_state_manager)),
        true,
    )).await?;

    info!("Starting Stream Deck application...");

    run(config, deck, render_config, theme, context, receiver)
        .await
        .map_err(|e| anyhow::anyhow!("StreamDeck error: {}", e))?;

    Ok(())
}
