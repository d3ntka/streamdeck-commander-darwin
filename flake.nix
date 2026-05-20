{
  description = "Stream Deck commander for Darwin";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

  outputs = { self, nixpkgs }:
    let
      mkPackage = { pkgs, embeddedConfig }:
        let
          configYaml = (pkgs.formats.yaml { }).generate "config.yaml" embeddedConfig;
        in
        pkgs.rustPlatform.buildRustPackage {
          pname = "streamdeck-commander";
          version = "0.1.0";
          src = self;
          cargoLock.lockFile = ./Cargo.lock;
          buildInputs = [ pkgs.hidapi ]
            ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [ pkgs.apple-sdk ];
          nativeBuildInputs = [ pkgs.pkg-config ];
          preBuild = ''
            cp ${configYaml} config.yaml
          '';
        };
    in
    {
      # Convenience target for local testing: nix build .#
      packages.aarch64-darwin.default = mkPackage {
        pkgs = nixpkgs.legacyPackages.aarch64-darwin;
        embeddedConfig.menu = {
          name = "Main";
          buttons = [
            {
              type = "command";
              name = "Say Hello";
              icon = "terminal";
              command = "/usr/bin/say";
              args = [ "hello from stream deck" ];
            }
            {
              type = "command";
              name = "Finder";
              icon = "folder";
              command = "/usr/bin/open";
              args = [ "-a" "Finder" ];
            }
            {
              type = "command";
              name = "Terminal";
              icon = "terminal";
              command = "/usr/bin/open";
              args = [ "-a" "Terminal" ];
            }
            {
              type = "menu";
              name = "Dev";
              icon = "code";
              buttons = [
                {
                  type = "command";
                  name = "Say Dev";
                  icon = "terminal";
                  command = "/usr/bin/say";
                  args = [ "you are in the dev menu" ];
                }
                {
                  type = "command";
                  name = "Safari";
                  icon = "language";
                  command = "/usr/bin/open";
                  args = [ "-a" "Safari" ];
                }
                {
                  type = "back";
                  icon = "arrow_back";
                }
              ];
            }
          ];
        };
      };

      lib.mkHaHelpers =
        { pkgs, haUrl, haToken }:
        let
          curl = "${pkgs.curl}/bin/curl";
          bash = "${pkgs.bash}/bin/bash";
          jq   = "${pkgs.jq}/bin/jq";
          authHeader = "Authorization: Bearer ${haToken}";
          mkHaToggle =
            { name
            , entityId
            , domain   ? "light"
            , onIcon   ? "light_mode"
            , offIcon  ? "light_off"
            }:
            {
              type = "toggle";
              inherit name onIcon offIcon;
              mode        = "separate";
              on_command  = curl;
              on_args     = [ "-sf" "-X" "POST"
                              "${haUrl}/api/services/${domain}/turn_on"
                              "-H" authHeader
                              "-H" "Content-Type: application/json"
                              "-d" "{\"entity_id\":\"${entityId}\"}" ];
              off_command = curl;
              off_args    = [ "-sf" "-X" "POST"
                              "${haUrl}/api/services/${domain}/turn_off"
                              "-H" authHeader
                              "-H" "Content-Type: application/json"
                              "-d" "{\"entity_id\":\"${entityId}\"}" ];
              probe_command = bash;
              probe_args    = [ "-c"
                                "${curl} -sf -H '${authHeader}' ${haUrl}/api/states/${entityId} | ${jq} -e '.state == \"on\"' > /dev/null" ];
            };
        in
        { inherit mkHaToggle; };

      homeManagerModules.default = { config, lib, pkgs, ... }:
        let
          cfg = config.programs.streamdeck-commander;
          package = mkPackage {
            inherit pkgs;
            embeddedConfig.menu = cfg.menu;
          };
        in
        {
          options.programs.streamdeck-commander = {
            enable = lib.mkEnableOption "Stream Deck commander";
            menu = lib.mkOption {
              type = lib.types.attrs;
              description = "Menu configuration — buttons and submenus.";
            };
          };

          config = lib.mkIf cfg.enable {
            launchd.agents.streamdeck-commander = {
              enable = true;
              config = {
                ProgramArguments = [ "${package}/bin/streamdeck-commander" ];
                RunAtLoad = true;
                KeepAlive = true;
                ThrottleInterval = 30;
                # Logs at /tmp/streamdeck-commander.log
                StandardOutPath = "/tmp/streamdeck-commander.log";
                StandardErrorPath = "/tmp/streamdeck-commander.log";
              };
            };
          };
        };
    };
}
