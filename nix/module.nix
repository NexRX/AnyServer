{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.services.anyserver;
  nginxCfg = cfg.nginx;
  inherit (lib)
    mkEnableOption
    mkOption
    mkIf
    mkDefault
    mkPackageOption
    types
    optionalAttrs
    optionalString
    ;
in
{
  options.services.anyserver = {
    enable = mkEnableOption "AnyServer, a self-hosted panel for running any binary as a managed server";

    package = mkPackageOption pkgs "anyserver" { };

    user = mkOption {
      type = types.str;
      default = "anyserver";
      description = "User account under which AnyServer runs.";
    };

    group = mkOption {
      type = types.str;
      default = "anyserver";
      description = "Group under which AnyServer runs.";
    };

    dataDir = mkOption {
      type = types.path;
      default = "/var/lib/anyserver";
      description = ''
        Directory where AnyServer stores all persistent data including the
        SQLite database, server files, SFTP host keys, and JWT secrets.
      '';
    };

    httpPort = mkOption {
      type = types.port;
      default = 3001;
      description = "Port for the HTTP API and WebSocket server.";
    };

    sftpPort = mkOption {
      type = types.port;
      default = 2222;
      description = "Port for the embedded SFTP server.";
    };

    jwtSecretFile = mkOption {
      type = types.nullOr types.path;
      default = null;
      description = ''
        Path to a file containing the JWT secret used for signing tokens.
        The file should contain a single line with the secret string.

        If not set, AnyServer will generate and persist a random key to
        `''${dataDir}/jwt_secret` on first start.

        For production use, it is recommended to set this to a stable
        secret managed outside of AnyServer (e.g. via agenix or sops-nix)
        so that sessions survive restarts.
      '';
      example = "/run/secrets/anyserver-jwt-secret";
    };

    corsOrigin = mkOption {
      type = types.nullOr types.str;
      default = null;
      description = ''
        Allowed CORS origin(s), comma-separated. Defaults to same-origin
        in production builds if unset.

        When {option}`services.anyserver.nginx.enable` is set, this
        defaults automatically to the configured domain with the
        appropriate scheme.
      '';
      example = "https://your-domain.example.com";
    };

    trustedProxies = mkOption {
      type = types.nullOr types.str;
      default = null;
      description = ''
        Trusted reverse-proxy IPs/CIDRs (e.g. "127.0.0.1,10.0.0.0/8").
        Required when running behind a reverse proxy so that
        X-Forwarded-For headers are respected.

        When {option}`services.anyserver.nginx.enable` is set, this
        defaults automatically to `"127.0.0.1,::1"`.
      '';
      example = "127.0.0.1,10.0.0.0/8,172.16.0.0/12";
    };

    cookieSecure = mkOption {
      type = types.enum [
        "auto"
        "true"
        "false"
      ];
      default = "auto";
      description = ''
        Controls the `Secure` flag on refresh cookies.
        - `"true"` — HTTPS only
        - `"false"` — plain HTTP allowed
        - `"auto"` — determined automatically based on build type

        When {option}`services.anyserver.nginx.forceSSL` is set, this
        defaults automatically to `"true"`.
      '';
    };

    csp = mkOption {
      type = types.nullOr types.str;
      default = null;
      description = ''
        Custom Content-Security-Policy header value. Set to an empty
        string to disable CSP entirely.
      '';
    };

    dbMaxConnections = mkOption {
      type = types.ints.positive;
      default = 16;
      description = "SQLite connection pool size.";
    };

    logLevel = mkOption {
      type = types.str;
      default = "anyserver=info,tower_http=info";
      description = ''
        Rust log filter string passed via the `RUST_LOG` environment
        variable. See the `tracing-subscriber` EnvFilter documentation
        for the full syntax.
      '';
      example = "anyserver=debug,tower_http=debug";
    };

    extraEnvironment = mkOption {
      type = types.attrsOf types.str;
      default = { };
      description = ''
        Extra environment variables to pass to the AnyServer process.
        These are set directly on the systemd service and can override
        any of the built-in variables.
      '';
      example = {
        ANYSERVER_DB_MAX_CONNECTIONS = "32";
      };
    };

    openFirewall = mkOption {
      type = types.bool;
      default = false;
      description = ''
        Whether to open ports in the firewall.

        When the nginx reverse proxy is **disabled**, this opens
        {option}`httpPort` and {option}`sftpPort`.

        When the nginx reverse proxy is **enabled**, this opens ports
        80, 443, and {option}`sftpPort` instead (the raw HTTP port is
        only accessed via localhost).
      '';
    };

    # ── Nginx reverse proxy ───────────────────────────────────
    nginx = {
      enable = mkEnableOption "an nginx reverse proxy in front of AnyServer";

      domain = mkOption {
        type = types.str;
        description = "Domain name for the nginx virtual host.";
        example = "servers.example.com";
      };

      forceSSL = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Whether to redirect all HTTP traffic to HTTPS.
          You almost always want this together with
          {option}`services.anyserver.nginx.enableACME`.
        '';
      };

      enableACME = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Whether to enable ACME (Let's Encrypt) certificate provisioning
          for the virtual host.
        '';
      };

      extraVirtualHostConfig = mkOption {
        type = types.attrsOf types.anything;
        default = { };
        description = ''
          Extra attributes merged into the
          `services.nginx.virtualHosts.<domain>` definition.
          Use this for anything not covered by the options above,
          such as `sslCertificate`, `basicAuth`, `listen`, etc.
        '';
        example = lib.literalExpression ''
          {
            basicAuthFile = "/run/secrets/anyserver-htpasswd";
            extraConfig = "client_max_body_size 512m;";
          }
        '';
      };
    };
  };

  config = mkIf cfg.enable {

    # ── Auto-wire settings when nginx is enabled ──────────────
    services.anyserver = mkIf nginxCfg.enable {
      trustedProxies = mkDefault "127.0.0.1,::1";
      cookieSecure = mkIf nginxCfg.forceSSL (mkDefault "true");
      corsOrigin =
        let
          scheme = if nginxCfg.forceSSL then "https" else "http";
        in
        mkDefault "${scheme}://${nginxCfg.domain}";
    };

    # ── Users & groups ────────────────────────────────────────
    users.users = optionalAttrs (cfg.user == "anyserver") {
      anyserver = {
        isSystemUser = true;
        inherit (cfg) group;
        home = cfg.dataDir;
        description = "AnyServer service user";
      };
    };

    users.groups = optionalAttrs (cfg.group == "anyserver") {
      anyserver = { };
    };

    # ── Firewall ──────────────────────────────────────────────
    networking.firewall = mkIf cfg.openFirewall {
      allowedTCPPorts =
        if nginxCfg.enable then
          # Behind nginx: expose HTTP(S) + SFTP, not the raw backend port
          [
            80
            443
            cfg.sftpPort
          ]
        else
          [
            cfg.httpPort
            cfg.sftpPort
          ];
    };

    # ── Nginx ─────────────────────────────────────────────────
    services.nginx = mkIf nginxCfg.enable {
      enable = true;
      recommendedProxySettings = true;
      recommendedTlsSettings = true;
      recommendedOptimisation = true;
      recommendedGzipSettings = true;

      virtualHosts.${nginxCfg.domain} = {
        forceSSL = nginxCfg.forceSSL;
        enableACME = nginxCfg.enableACME;

        locations."/" = {
          proxyPass = "http://127.0.0.1:${toString cfg.httpPort}";
          proxyWebsockets = true;
          extraConfig = ''
            # AnyServer streams the live console over long-lived
            # WebSocket connections — disable proxy buffering and
            # use a generous read timeout so idle connections are
            # not dropped.
            proxy_buffering off;
            proxy_read_timeout 86400s;
            proxy_send_timeout 86400s;
          '';
        };
      }
      // nginxCfg.extraVirtualHostConfig;
    };

    # ── Systemd service ───────────────────────────────────────
    systemd.services.anyserver = {
      description = "AnyServer — self-hosted server management panel";
      after = [ "network.target" ] ++ lib.optional nginxCfg.enable "nginx.service";
      wantedBy = [ "multi-user.target" ];

      path = [
        cfg.package
        config.system.path
      ];

      environment = {
        ANYSERVER_DATA_DIR = cfg.dataDir;
        ANYSERVER_HTTP_PORT = toString cfg.httpPort;
        ANYSERVER_SFTP_PORT = toString cfg.sftpPort;
        ANYSERVER_COOKIE_SECURE = cfg.cookieSecure;
        ANYSERVER_DB_MAX_CONNECTIONS = toString cfg.dbMaxConnections;
        RUST_LOG = cfg.logLevel;
      }
      // optionalAttrs (cfg.corsOrigin != null) {
        ANYSERVER_CORS_ORIGIN = cfg.corsOrigin;
      }
      // optionalAttrs (cfg.trustedProxies != null) {
        ANYSERVER_TRUSTED_PROXIES = cfg.trustedProxies;
      }
      // optionalAttrs (cfg.csp != null) {
        ANYSERVER_CSP = cfg.csp;
      }
      // cfg.extraEnvironment;

      script = ''
        ${optionalString (cfg.jwtSecretFile != null) ''
          export ANYSERVER_JWT_SECRET="$(cat ${lib.escapeShellArg cfg.jwtSecretFile})"
        ''}
        exec ${lib.getExe cfg.package}
      '';

      serviceConfig = {
        Type = "simple";
        User = cfg.user;
        Group = cfg.group;
        WorkingDirectory = cfg.dataDir;
        StateDirectory = lib.removePrefix "/var/lib/" cfg.dataDir;
        StateDirectoryMode = "0750";
        Restart = "on-failure";
        RestartSec = 5;
        TimeoutStopSec = 30;

        # Hardening
        #
        # AnyServer spawns and manages child processes (game servers, etc.)
        # that may require JIT compilation (Java, .NET, V8) and namespace
        # isolation, so we keep some settings permissive while still
        # locking down what we can.
        NoNewPrivileges = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        PrivateTmp = true;
        PrivateDevices = true;
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectKernelLogs = true;
        ProtectControlGroups = true;
        ProtectClock = true;
        ProtectHostname = true;
        RestrictRealtime = true;
        RestrictSUIDSGID = true;
        LockPersonality = true;
        RemoveIPC = true;
        SystemCallArchitectures = "native";
        SystemCallFilter = [
          "@system-service"
          "@resources"
        ];

        # Managed servers (Java/Minecraft, .NET/Terraria, etc.) need JIT
        # compilation, which requires writable+executable memory mappings.
        MemoryDenyWriteExecute = false;

        # AnyServer supports optional namespace-based process sandboxing
        # for managed servers, so we cannot restrict namespace creation.
        RestrictNamespaces = false;

        # Allow binding to the configured ports
        AmbientCapabilities = mkIf (cfg.httpPort < 1024 || cfg.sftpPort < 1024) [
          "CAP_NET_BIND_SERVICE"
        ];
        CapabilityBoundingSet = mkIf (cfg.httpPort < 1024 || cfg.sftpPort < 1024) [
          "CAP_NET_BIND_SERVICE"
        ];

        # Read-write access to the data directory, read-only to /proc for sysinfo
        ReadWritePaths = [ cfg.dataDir ];
        SupplementaryGroups = [ ];

        # sysinfo crate needs access to /proc and /sys for system metrics
        ProcSubset = "all";
      };
    };
  };
}
