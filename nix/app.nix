{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.axismundi;
  helpers = import ./lib.nix { inherit lib pkgs; };
  inherit (helpers)
    runtimeDir
    appUser
    appGroup
    pgPackage
    ;
  rt = helpers.mkRuntime cfg;
  inherit (rt)
    isPackageSource
    appPackage
    minioHostPort
    lexurgyHostPort
    ;
  inherit (lib)
    mkIf
    mkDefault
    optional
    ;
in
{
  config = mkIf cfg.enable {
    # in package mode the app talks to the supporting containers over
    # 127.0.0.1, not the podman network
    services.axismundi.config.s3.endpoint = mkIf (isPackageSource && cfg.minio.enable) (
      mkDefault "http://127.0.0.1:${toString minioHostPort}"
    );
    services.axismundi.config.lexurgy.url = mkIf (isPackageSource && cfg.lexurgy.enable) (
      mkDefault "http://127.0.0.1:${toString lexurgyHostPort}"
    );

    users.users.${appUser} = mkIf isPackageSource {
      isSystemUser = true;
      group = appGroup;
      description = "axismundi web app";
    };
    users.groups.${appGroup} = mkIf isPackageSource { };

    systemd.services.axismundi-migrate = mkIf (isPackageSource && cfg.postgres.enable) {
      description = "apply axismundi sqlx migrations";
      wantedBy = [ "multi-user.target" ];
      after = [
        "axismundi-config.service"
        "podman-axismundi-postgres.service"
      ];
      before = [ "axismundi.service" ];
      requires = [
        "axismundi-config.service"
        "podman-axismundi-postgres.service"
      ];
      restartTriggers = [ appPackage ];
      path = [
        pgPackage
        pkgs.jq
        pkgs.sqlx-cli
        pkgs.coreutils
      ];
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
      };
      script = ''
        db_url=$(jq -r .database_url ${runtimeDir}/config.json)
        # postgres-cmd-ready != accepting-app-connections; pg_isready first
        # avoids a flurry of "the database system is starting up" failures.
        for i in $(seq 1 60); do
          if pg_isready -d "$db_url" -q; then break; fi
          sleep 1
        done
        cd ${appPackage}/share/axismundi
        DATABASE_URL="$db_url" sqlx migrate run --source migrations
      '';
    };

    systemd.services.axismundi = mkIf isPackageSource {
      description = "axismundi web app";
      wantedBy = [ "multi-user.target" ];
      after =
        [
          "network.target"
          "axismundi-config.service"
        ]
        ++ optional cfg.postgres.enable "podman-axismundi-postgres.service"
        ++ optional cfg.postgres.enable "axismundi-migrate.service"
        ++ optional cfg.minio.enable "podman-axismundi-minio.service"
        ++ optional cfg.lexurgy.enable "podman-axismundi-lexurgy.service";
      requires =
        [ "axismundi-config.service" ]
        ++ optional cfg.postgres.enable "axismundi-migrate.service";
      wants =
        optional cfg.postgres.enable "podman-axismundi-postgres.service"
        ++ optional cfg.minio.enable "podman-axismundi-minio.service"
        ++ optional cfg.lexurgy.enable "podman-axismundi-lexurgy.service";
      restartTriggers = [ appPackage ];
      serviceConfig = {
        Type = "simple";
        User = appUser;
        Group = appGroup;
        EnvironmentFile = optional (cfg.envFile != null) cfg.envFile;
        ExecStart = "${appPackage}/bin/axismundi ${runtimeDir}/config.json";
        Restart = "on-failure";
        RestartSec = "5s";
        NoNewPrivileges = true;
        PrivateTmp = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectControlGroups = true;
        RestrictAddressFamilies = [
          "AF_INET"
          "AF_INET6"
          "AF_UNIX"
        ];
      };
    };
  };
}
