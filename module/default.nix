# Codesearch home-manager module — daemon + MCP server entry
#
# Namespace: services.codesearch.daemon.* / services.codesearch.mcp.*
#
# The daemon manages all repos via a single process reading a YAML config.
# The MCP entry exposes a serverEntry attrset for consumption by claude modules.
#
{
  lib,
  config,
  pkgs,
  ...
}:
with lib; let
  daemonCfg = config.services.codesearch.daemon;
  mcpCfg = config.services.codesearch.mcp;
  githubCfg = daemonCfg.github;
  isDarwin = pkgs.stdenv.isDarwin;

  # ── Daemon YAML config (generated from nix options) ──────────────────
  codesearchDaemonConfig = pkgs.writeText "codesearch-daemon.yaml"
    (builtins.toJSON ({
      port = daemonCfg.port;
      index_interval = daemonCfg.indexInterval;
      lmdb_map_size_mb = daemonCfg.lmdbMapSizeMB;
      repos = daemonCfg.repos;
    } // optionalAttrs (daemonCfg.model != null) {
      model = daemonCfg.model;
    } // optionalAttrs githubCfg.enable {
      github = {
        sources = map (s: {
          owner = s.owner;
          kind = s.kind;
          clone_base = s.cloneBase;
          auto_clone = s.autoClone;
          skip_archived = s.skipArchived;
          skip_forks = s.skipForks;
          exclude = s.exclude;
        }) githubCfg.sources;
      } // optionalAttrs (githubCfg.tokenFile != null) {
        token_file = githubCfg.tokenFile;
      };
    }));
in {
  options.services.codesearch = {
    # ── Daemon options ─────────────────────────────────────────────────
    daemon = {
      enable = mkOption {
        type = types.bool;
        default = false;
        description = "Enable Codesearch daemon (semantic code search with live file watching)";
      };

      package = mkOption {
        type = types.package;
        default = pkgs.codesearch;
        description = "codesearch package providing codesearch binary";
      };

      repos = mkOption {
        type = types.listOf types.str;
        default = [];
        description = "Repository paths to index (e.g. [\"/home/user/code/myrepo\"])";
      };

      model = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Embedding model to use (null = codesearch default mxbai-embed-xsmall-v1). Options: minilm-l6, bge-small, jina-code, etc.";
      };

      port = mkOption {
        type = types.int;
        default = 4444;
        description = "Codesearch serve port (HTTP API for search)";
      };

      lmdbMapSizeMB = mkOption {
        type = types.int;
        default = 2048;
        description = "LMDB map size in MB. Default 2GB handles large monorepos.";
      };

      indexInterval = mkOption {
        type = types.int;
        default = 300;
        description = "Periodic re-index interval in seconds for the daemon (default 5 minutes)";
      };

      github = {
        enable = mkOption {
          type = types.bool;
          default = false;
          description = "Enable GitHub auto-discovery of repos (list org/user repos, optionally clone missing)";
        };

        tokenFile = mkOption {
          type = types.nullOr types.str;
          default = null;
          description = "Path to file containing GitHub token. Falls back to GITHUB_TOKEN env var.";
        };

        sources = mkOption {
          type = types.listOf (types.submodule {
            options = {
              owner = mkOption {
                type = types.str;
                description = "GitHub owner name (org or username)";
              };
              kind = mkOption {
                type = types.enum ["org" "user"];
                default = "org";
                description = "Whether this is an organization or user account";
              };
              cloneBase = mkOption {
                type = types.str;
                description = "Local directory where repos are/should be cloned";
              };
              autoClone = mkOption {
                type = types.bool;
                default = false;
                description = "Automatically clone repos that don't exist locally";
              };
              skipArchived = mkOption {
                type = types.bool;
                default = true;
                description = "Skip archived repositories";
              };
              skipForks = mkOption {
                type = types.bool;
                default = false;
                description = "Skip forked repositories";
              };
              exclude = mkOption {
                type = types.listOf types.str;
                default = [];
                description = "Glob patterns to exclude repo names (e.g. [\"*.wiki\" \"legacy-*\"])";
              };
            };
          });
          default = [];
          description = "GitHub sources to discover repos from";
        };
      };
    };

    # ── MCP options ────────────────────────────────────────────────────
    mcp = {
      enable = mkOption {
        type = types.bool;
        default = false;
        description = "Generate MCP server entry for codesearch (consumed by claude modules)";
      };

      package = mkOption {
        type = types.package;
        default = daemonCfg.package;
        defaultText = literalExpression "config.services.codesearch.daemon.package";
        description = "codesearch package for MCP server binary";
      };

      serverEntry = mkOption {
        type = types.attrs;
        default = {};
        internal = true;
        readOnly = true;
        description = "Generated MCP server attrset — consumed by claude module, not set by users";
      };
    };
  };

  # ── Config ─────────────────────────────────────────────────────────
  config = mkMerge [
    # MCP server entry (always generated when mcp.enable is true)
    (mkIf mcpCfg.enable {
      services.codesearch.mcp.serverEntry = {
        type = "stdio";
        command = "${mcpCfg.package}/bin/codesearch";
        args = ["mcp"];
        env = {
          CODESEARCH_LMDB_MAP_SIZE_MB = toString daemonCfg.lmdbMapSizeMB;
        };
      };
    })

    # Darwin: launchd agent for codesearch daemon
    (mkIf (daemonCfg.enable && isDarwin && (daemonCfg.repos != [] || githubCfg.enable)) {
      launchd.agents.codesearch-daemon = {
        enable = true;
        config = {
          Label = "io.pleme.codesearch-daemon";
          ProgramArguments = [
            "${daemonCfg.package}/bin/codesearch"
            "daemon"
            "--config"
            "${codesearchDaemonConfig}"
          ];
          EnvironmentVariables = {
            CODESEARCH_LMDB_MAP_SIZE_MB = toString daemonCfg.lmdbMapSizeMB;
          };
          RunAtLoad = true;
          KeepAlive = true;
          ProcessType = "Adaptive";
          StandardOutPath = "${config.home.homeDirectory}/Library/Logs/codesearch-daemon.log";
          StandardErrorPath = "${config.home.homeDirectory}/Library/Logs/codesearch-daemon.err";
        };
      };
    })

    # Linux: systemd user service for codesearch daemon
    (mkIf (daemonCfg.enable && !isDarwin && (daemonCfg.repos != [] || githubCfg.enable)) {
      systemd.user.services.codesearch-daemon = {
        Unit = {
          Description = "Codesearch daemon — semantic code search";
          After = ["default.target"];
        };
        Service = {
          Type = "simple";
          ExecStart = "${daemonCfg.package}/bin/codesearch daemon --config ${codesearchDaemonConfig}";
          Environment = "CODESEARCH_LMDB_MAP_SIZE_MB=${toString daemonCfg.lmdbMapSizeMB}";
          Restart = "on-failure";
          RestartSec = 5;
        };
        Install.WantedBy = ["default.target"];
      };
    })
  ];
}
