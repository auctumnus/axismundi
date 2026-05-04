{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.axismundi;
  helpers = import ./lib.nix { inherit lib pkgs; };
  inherit (helpers) secretSubmodule mkSecretAssert;
  rt = helpers.mkRuntime cfg;
  inherit (rt) isLocalSource;
  inherit (lib)
    mkDefault
    mkIf
    mkOption
    mkEnableOption
    optional
    types
    ;
in
{
  imports = [
    ./config.nix
    ./containers.nix
    ./app.nix
    ./backup.nix
    ./caddy.nix
  ];

  options.services.axismundi = {
    enable = mkEnableOption "axismundi";

    source = mkOption {
      description = ''
        where the app comes from. one of:

        - `"local"`: expect `axismundi:local` to already exist in podman's
          image store. deploys are decoupled from `nixos-rebuild`:
              podman build -t axismundi:local .
              sudo systemctl restart podman-axismundi.service
          the system flake doesn't track app version.

        - `{ registry = "..."; tag = "..."; }`: container image pulled
          from a registry on activation. system flake tracks the
          version; deploys are a tag bump + `nixos-rebuild switch`.

        - `{ package = <derivation>; }`: nix-built binary, run as a
          regular systemd service on the host (no app container). the
          supporting services (postgres, minio, lexurgy) still run as
          podman containers, but their ports get published on 127.0.0.1
          so the host-side app can reach them. usually fed
          `self.packages.''${system}.axismundi` from the system flake.
      '';
      type = types.either (types.enum [ "local" ]) (
        types.submodule {
          options = {
            registry = mkOption {
              type = types.nullOr types.str;
              default = null;
              example = "ghcr.io/auctumnus/axismundi";
            };
            tag = mkOption {
              type = types.nullOr types.str;
              default = null;
              example = "v1.4.2";
            };
            package = mkOption {
              type = types.nullOr types.package;
              default = null;
              description = ''
                nix-built axismundi derivation. mutually exclusive with
                registry/tag. when set, the app runs as a host systemd
                service instead of a podman container.
              '';
            };
          };
        }
      );
      default = "local";
    };

    envFile = mkOption {
      type = types.nullOr types.path;
      default = null;
      description = ''
        optional environment file for the app container, for env vars that
        aren't part of config.json (e.g. RUST_LOG, RUST_BACKTRACE). config
        values belong in `services.axismundi.config.*`.
      '';
    };

    stateDir = mkOption {
      type = types.path;
      default = "/var/lib/axismundi";
      description = "where postgres + minio volumes live";
    };

    backupDir = mkOption {
      type = types.path;
      default = "/var/lib/axismundi/backups";
    };

    # ----- the app's runtime config (mirrors AppConfig in src/config.rs) ---

    config = {
      port = mkOption {
        type = types.port;
        default = 3000;
        description = "tcp port the app listens on, inside the container AND on the host";
      };
      publicUrlBase = mkOption {
        type = types.str;
        example = "https://axismundi.app";
        description = "external base URL the app is served at (used in emails, og tags, etc)";
      };
      environment = mkOption {
        type = types.enum [
          "Dev"
          "Prod"
        ];
        default = "Prod";
      };
      fileUploadLimitBytes = mkOption {
        type = types.int;
        default = 5 * 1024 * 1024;
      };

      s3 = {
        bucket = mkOption {
          type = types.str;
          default = "axismundi";
        };
        region = mkOption {
          type = types.str;
          default = "us-east-1";
        };
        endpoint = mkOption {
          type = types.str;
          default = "http://axismundi-minio:9000";
          description = "s3 endpoint URL the app uses to talk to minio (internal podman address by default)";
        };
        publicUrlBase = mkOption {
          type = types.nullOr types.str;
          default = null;
          example = "https://media.axismundi.app";
          description = ''
            external URL where imagor is served — used by the app to
            construct image URLs in HTML / JSON responses. usually
            matches `caddy.mediaDomain` (with `https://`).
          '';
        };
        accessKey = secretSubmodule ''
          s3 access key. used by the app's s3 client, AND by minio as
          MINIO_ROOT_USER, AND by imagor as AWS_ACCESS_KEY_ID. one source
          of truth — the module wires it through to all three.
        '';
        secretKey = secretSubmodule ''
          s3 secret key. counterpart to accessKey across the same three
          components.
        '';
        imagorSecret = secretSubmodule ''
          HMAC key for signing imagor URLs (set in production). shared
          between the app (signs) and the imagor container (verifies).
          if unset, imagor runs with IMAGOR_UNSAFE=1, which is DEV ONLY:
          anyone can request arbitrary transforms on arbitrary URLs.
        '';
      };

      lexurgy = {
        url = mkOption {
          type = types.str;
          default = "http://axismundi-lexurgy:8080";
          description = "lexurgy service URL the app talks to";
        };
        apiKey = secretSubmodule ''
          lexurgy api key. shared between the app (.lexurgy.api_key in
          config.json) and the lexurgy container's API_KEY env var.
        '';
      };

      email = mkOption {
        default = "mock";
        description = ''
          email backend.
          - `"mock"`: emails logged but not sent. fine for dev.
          - `{ resend = { ... }; }`: send via the resend api.
        '';
        type = types.either (types.enum [ "mock" ]) (
          types.submodule {
            options = {
              resend = {
                apiKey = secretSubmodule "resend api key";
                fromEmail = mkOption {
                  type = types.str;
                  example = "noreply@axismundi.app";
                };
              };
            };
          }
        );
      };

      banner = {
        enabled = mkOption {
          type = types.bool;
          default = false;
        };
        message = mkOption {
          type = types.str;
          default = "";
        };
        kind = mkOption {
          type = types.str;
          default = "info";
          description = "banner severity class (e.g. info, warn, error)";
        };
      };

      maid = {
        port = mkOption {
          type = types.port;
          default = 3003;
        };
        healthCheckTimeoutMs = mkOption {
          type = types.int;
          default = 10000;
        };
        waitBetweenTasksMs = mkOption {
          type = types.int;
          default = 1000;
        };
        taskTimeoutMs = mkOption {
          type = types.int;
          default = 15000;
        };
      };
    };

    # ----- supporting services -----

    postgres = {
      enable = mkOption {
        type = types.bool;
        default = true;
        description = ''
          run postgres as a sibling container. set false to use an
          external db; in that case set postgres.databaseUrl instead of
          postgres.password.
        '';
      };
      password = secretSubmodule ''
        postgres password (used when postgres.enable = true). drives both
        the local container's POSTGRES_PASSWORD and the app's
        DATABASE_URL.
      '';
      databaseUrl = secretSubmodule ''
        full DATABASE_URL with embedded creds (used when
        postgres.enable = false). bypasses URL construction; the app
        connects directly to whatever this points at.
      '';
      hostPort = mkOption {
        type = types.nullOr types.port;
        default = null;
        description = ''
          host port (bound to 127.0.0.1) where the postgres container
          publishes 5432. null means don't publish — postgres is only
          reachable over the podman network. defaults to 5432 when
          source is a package (the host-side app needs a way in).
        '';
      };
    };

    minio = {
      enable = mkOption {
        type = types.bool;
        default = true;
      };
      bucket = mkOption {
        type = types.str;
        default = "axismundi";
      };
      hostPort = mkOption {
        type = types.nullOr types.port;
        default = null;
        description = ''
          host port (bound to 127.0.0.1) where the minio container
          publishes 9000. null means don't publish. defaults to 9000
          when source is a package.
        '';
      };
    };

    imagor = {
      enable = mkOption {
        type = types.bool;
        default = true;
      };
      port = mkOption {
        type = types.port;
        default = 8888;
        description = ''
          host port (bound to 127.0.0.1 only) where imagor is reachable.
          used by caddy / any host-side reverse proxy on the media
          subdomain. internal-only — never publish directly.
        '';
      };
    };

    lexurgy = {
      enable = mkOption {
        type = types.bool;
        default = true;
      };
      hostPort = mkOption {
        type = types.nullOr types.port;
        default = null;
        description = ''
          host port (bound to 127.0.0.1) where the lexurgy container
          publishes 8080. null means don't publish. defaults to 8080
          when source is a package.
        '';
      };
    };

    caddy = {
      enable = mkOption {
        type = types.bool;
        default = false;
        description = "manage caddy virtualhosts for the app + media subdomain.";
      };
      domain = mkOption {
        type = types.str;
        example = "axismundi.app";
      };
      mediaDomain = mkOption {
        type = types.nullOr types.str;
        default = null;
        example = "media.axismundi.app";
        description = "if set, adds a vhost proxying to imagor. requires imagor.enable.";
      };
      fallbackPage = mkOption {
        type = types.path;
        default = ./fallback.html;
        description = "html shown when the upstream is unreachable (connection-refused / dial timeout).";
      };
      fallbackPort = mkOption {
        type = types.port;
        default = 8081;
        description = "internal loopback port where caddy serves the fallback page.";
      };
      extraConfig = mkOption {
        type = types.lines;
        default = "";
        description = "extra caddyfile directives appended inside the main site block";
      };
    };

    backup = {
      enable = mkOption {
        type = types.bool;
        default = true;
      };
      schedule = mkOption {
        type = types.str;
        default = "daily";
        description = "systemd OnCalendar expression for local backups";
      };
      offsite = {
        enable = mkOption {
          type = types.bool;
          default = false;
        };
        configFile = mkOption {
          type = types.nullOr types.path;
          default = null;
          description = ''
            path to backup.json (b2 + age recipient). NOT in the nix
            store. mode 0400 root recommended.
          '';
        };
        schedule = mkOption {
          type = types.str;
          default = "daily";
        };
      };
    };
  };

  config = mkIf cfg.enable {
    assertions =
      [
        (mkSecretAssert "postgres.password" cfg.postgres.enable cfg.postgres.password)
        (mkSecretAssert "postgres.databaseUrl" (!cfg.postgres.enable) cfg.postgres.databaseUrl)
        (mkSecretAssert "config.s3.accessKey" true cfg.config.s3.accessKey)
        (mkSecretAssert "config.s3.secretKey" true cfg.config.s3.secretKey)
        (mkSecretAssert "config.s3.imagorSecret" false cfg.config.s3.imagorSecret)
        (mkSecretAssert "config.lexurgy.apiKey" true cfg.config.lexurgy.apiKey)
      ]
      ++ optional (cfg.config.email != "mock") (
        mkSecretAssert "config.email.resend.apiKey" true cfg.config.email.resend.apiKey
      )
      ++ [
        {
          assertion = !cfg.backup.offsite.enable || cfg.backup.offsite.configFile != null;
          message = "services.axismundi.backup.offsite.enable requires backup.offsite.configFile";
        }
        {
          assertion = cfg.caddy.mediaDomain == null || cfg.imagor.enable;
          message = "services.axismundi.caddy.mediaDomain requires imagor.enable = true";
        }
        {
          # the app's s3 init unconditionally `.expect()`s public_url_base
          # to be set (src/util/s3.rs). letting it default to null gets
          # you a panic on first request, which is a really bad failure
          # mode — fail at evaluation instead.
          assertion = cfg.config.s3.publicUrlBase != null;
          message = ''
            services.axismundi.config.s3.publicUrlBase must be set —
            this is the external URL where imagor is served (used by
            the app to construct image URLs in HTML / JSON responses).
            usually matches `caddy.mediaDomain` (with `https://`).
          '';
        }
        {
          assertion =
            isLocalSource
            || (cfg.source.package != null && cfg.source.registry == null && cfg.source.tag == null)
            || (cfg.source.package == null && cfg.source.registry != null && cfg.source.tag != null);
          message = ''
            services.axismundi.source: when not "local", set exactly one of
            { registry = ...; tag = ...; } or { package = ...; }.
          '';
        }
      ];

    services.journald.extraConfig = mkDefault ''
      MaxRetentionSec=30day
      SystemMaxUse=2G
    '';
  };
}
