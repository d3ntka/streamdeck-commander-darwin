# streamdeck-commander-darwin — Handoff

## What this is

A Darwin/macOS port of [skdziwak/streamdeck-nix](https://github.com/skdziwak/streamdeck-nix).

The upstream project provides a Rust binary (`streamdeck-commander`) that drives an Elgato Stream Deck device using a declarative Nix config — you define buttons and menus directly in Nix, Nix generates YAML at build time, bakes it into the binary at compile time, and a systemd service keeps it running. It's fully declarative: no GUI, no Elgato app needed.

**The problem:** it's NixOS-only. The Nix module uses `systemd.services`, `udev` rules, and X11/Wayland env vars. None of that exists on macOS.

**The goal:** make it work on macOS via a home-manager module that uses `launchd.agents` instead.

---

## Why it's worth doing

- Drop the official Elgato app entirely
- Stream Deck config lives in Nix alongside everything else — survives system rebuilds, wipes, new machines
- CLI commands and shell scripts as button actions — perfect for a developer shortcut console
- Nested menus, toggle buttons (on/off state with probe command), Material Design icons built in

---

## Upstream codebase

Clone and read for reference — do NOT develop in place:

```
https://github.com/skdziwak/streamdeck-nix
```

### How the config embedding works

The clever part: the Nix module takes your button config (defined as Nix attrsets), converts it to YAML via `pkgs.formats.yaml {}`, then in `preBuild` copies it to `config.yaml` which the Rust binary reads and embeds at compile time via `include_str!`. Result: a self-contained binary with your config baked in. No config file at runtime.

This means each config change triggers a `nhd` rebuild — which is fine, that's the Nix workflow.

### Key files in upstream

| File | Purpose |
|---|---|
| `flake.nix` | Package build + NixOS module — **main thing to port** |
| `src/main.rs` | Connects to device, loads embedded config, runs event loop |
| `src/config.rs` | YAML config deserialization |
| `src/button.rs` | Button rendering and action dispatch |
| `Cargo.toml` | Dependencies (streamdeck-oxide, tokio, serde_yaml, etc.) |

### Rust deps

```toml
streamdeck-oxide = { version = "0.2.1", features = ["plugins"] }
elgato-streamdeck  # pulled transitively by streamdeck-oxide
hidapi             # cross-platform HID — works on macOS via IOKit
tokio, serde, serde_yaml, anyhow, tracing
```

No Linux-specific Rust code anywhere. The binary compiles and runs on macOS as-is.

---

## What's Linux-only and needs replacing

### 1. Build inputs in `flake.nix`

```nix
# Upstream (Linux)
buildInputs = with pkgs; [ systemd hidapi udev ];

# Darwin replacement
buildInputs = with pkgs; [ hidapi ]
  ++ lib.optionals stdenv.isDarwin (with darwin.apple_sdk.frameworks; [ IOKit CoreFoundation ]);
```

### 2. NixOS module → home-manager module

Upstream exports `nixosModules.default` which uses `systemd.services`. 

Replace with a `homeManagerModules.default` that uses `launchd.agents`:

```nix
# Upstream
systemd.services.streamdeck-commander = {
  description = "StreamDeck Commander";
  after = [ "graphical-session.target" ];
  wantedBy = [ "default.target" ];
  serviceConfig = { ExecStart = ...; Restart = "on-failure"; ... };
};

# Darwin target
launchd.agents.streamdeck-commander = {
  enable = true;
  config = {
    ProgramArguments = [ "${package}/bin/streamdeck-commander" ];
    RunAtLoad = true;
    KeepAlive = true;
    StandardOutPath = "/tmp/streamdeck-commander.log";
    StandardErrorPath = "/tmp/streamdeck-commander.log";
  };
};
```

### 3. Linux env vars — drop entirely

The upstream wrapper script sets `DISPLAY`, `WAYLAND_DISPLAY`, `XDG_RUNTIME_DIR`, `DBUS_SESSION_BUS_ADDRESS`, `HYPRLAND_INSTANCE_SIGNATURE`. None of these exist or are needed on macOS.

### 4. udev rules — not needed

macOS handles HID device access natively. No udev rules, no group membership required. The binary can access the Stream Deck directly.

---

## Target module API

The home-manager module should expose the same button config options as upstream:

```nix
programs.streamdeck-commander = {
  enable = true;
  menu = {
    name = "Main";
    buttons = [
      {
        type = "command";
        name = "Neovim";
        command = "kitty";
        args = [ "-e" "nvim" ];
        icon = "terminal";
      }
      {
        type = "menu";
        name = "Git";
        icon = "code";
        buttons = [
          {
            type = "command";
            name = "Lazygit";
            command = "kitty";
            args = [ "-e" "lazygit" ];
            icon = "build";
          }
          { type = "back"; icon = "arrow_back"; }
        ];
      }
      {
        type = "toggle";
        name = "VPN";
        mode = "separate";
        on_command = "wg-quick";
        on_args = [ "up" "wg0" ];
        off_command = "wg-quick";
        off_args = [ "down" "wg0" ];
        probe_command = "test";
        probe_args = [ "-d" "/proc/sys/net/ipv4/conf/wg0" ];
        on_icon = "lock";
        off_icon = "lock_open";
      }
    ];
  };
};
```

---

## Suggested flake structure

```
streamdeck-darwin/
├── flake.nix          # package + homeManagerModules.default
├── src/               # Rust source (copy from upstream, no changes expected)
├── Cargo.toml
├── Cargo.lock
├── build.rs
└── icons/             # icon data (copy from upstream)
```

The flake should:
1. Build the Rust package with Darwin-appropriate build inputs
2. Export `homeManagerModules.default` with the `programs.streamdeck-commander` option set
3. Use `mkStreamDeckCommander { embeddedConfig = ...; }` pattern from upstream to bake config in at build time

---

## Known limitations

| Limitation | Details |
|---|---|
| Idle wake only on action buttons | Empty grid positions and Menu/Back buttons don't send on the activity channel, so pressing them won't wake the deck from idle sleep. The first actionable button press (Command or Toggle) does wake it. Fix: store `activity_sender` on `CommanderPlugin`, fill empty cells with invisible no-op `ClickButton`s — medium effort, deferred. |
| Idle sleep is brightness-only | `set_brightness(0)` dims the screen but doesn't issue a hardware sleep command. The device stays electrically active. Good enough for the use case. |
| Mac sleep not detected | The deck stays lit when macOS sleeps. Would require IOKit power management notifications (`kIOMessageSystemWillSleep`) — medium complexity, not yet implemented. |
| HA token baked into binary | The Home Assistant token is embedded at compile time via `include_str!`. It ends up in the Nix store (world-readable). Acceptable for a LAN-only token on a personal machine; not suitable for sensitive credentials. |

---

## Hardware note

The upstream binary hardcodes Mk2 (`U5, U3` — 5 columns × 3 rows) in `main.rs`:

```rust
generic_array::typenum::{U3, U5}
// and later:
run_with_external_triggers::<PluginNavigation<U5, U3>, U5, U3, PluginContext>(...)
```

**Resolved:** User has MK1 5×3. Grid dimensions match (`U5`/`U3` = 5×3) — no generic change needed. Device selection already has MK1 fallback: `list_devices` prefers MK2 but falls back to any connected device.

---

## Design decisions (resolved)

| Decision | Choice | Reason |
|---|---|---|
| Flake architecture | Copy source | Upstream is a one-off, no ongoing commits — nothing to track from a flake input |
| Rust binary changes | Device-wait loop patch only | Binary is otherwise Darwin-compatible as-is |
| Device-wait behavior | Poll every 5s, log once on start | Laptop moves away from desk regularly — crash+restart loop is wrong behavior |
| `ThrottleInterval` | 30s | Guards against genuine crash loops |
| PATH / commands | Fully-qualified paths (`${pkgs.kitty}/bin/kitty`) | Config is Nix — use Nix to resolve. Helper function later if verbosity is painful |
| Module options | `enable` + `menu` only | Drop all Linux-specific options; `user` implicit from home-manager |
| Log file | `/tmp/streamdeck-commander.log` hardcoded | Personal-only flake, no need to expose as option. Comment in module marks the path |

---

## Context: where this will be consumed

Once built, it will be added as a flake input to `~/.config/nix-den` (the user's nix-darwin + home-manager config, managed via the `den` framework). The module will be imported in the Hermes host config (macOS machine). Pattern for adding a flake input there is documented in `modules/homebrew.nix` and `flake.nix` in nix-den.

The user's Nix deploy command is `nhd` (alias for `nh darwin switch`).
