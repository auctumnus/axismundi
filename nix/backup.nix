{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.axismundi;
  helpers = import ./lib.nix { inherit lib pkgs; };
  inherit (helpers) runtimeDir pgPackage scriptsSrc;
  inherit (lib) mkIf;

  backupScript = pkgs.writeShellApplication {
    name = "axismundi-backup";
    runtimeInputs = [
      pgPackage
      pkgs.jq
      pkgs.coreutils
      pkgs.bash
      # backup-db.sh falls back to running pg_dump in a podman container on
      # the axismundi network when the db host (e.g. axismundi-postgres)
      # only resolves via podman-internal dns.
      pkgs.podman
    ];
    text = ''
      export AXISMUNDI_CONFIG=${runtimeDir}/config.json
      export AXISMUNDI_BACKUPS_DIR=${cfg.backupDir}
      exec ${pkgs.bash}/bin/bash ${scriptsSrc}/backup-db.sh
    '';
  };

  offsiteScript = pkgs.writeShellApplication {
    name = "axismundi-backup-offsite";
    runtimeInputs = [
      pkgs.age
      pkgs.rclone
      pkgs.jq
      pkgs.coreutils
      pkgs.curl
      pkgs.bash
    ];
    text = ''
      export AXISMUNDI_CONFIG=${runtimeDir}/config.json
      export AXISMUNDI_BACKUPS_DIR=${cfg.backupDir}
      export AXISMUNDI_BACKUP_CONFIG=${cfg.backup.offsite.configFile}
      exec ${pkgs.bash}/bin/bash ${scriptsSrc}/backup-offsite.sh
    '';
  };
in
{
  config = mkIf cfg.enable {
    systemd.services.axismundi-backup = mkIf cfg.backup.enable {
      description = "axismundi: local postgres backup";
      after = [
        "podman-axismundi-postgres.service"
        "axismundi-config.service"
      ];
      serviceConfig = {
        Type = "oneshot";
        ExecStart = "${backupScript}/bin/axismundi-backup";
        SuccessExitStatus = "0";
      };
    };

    systemd.timers.axismundi-backup = mkIf cfg.backup.enable {
      description = "axismundi: local backup timer";
      wantedBy = [ "timers.target" ];
      timerConfig = {
        OnCalendar = cfg.backup.schedule;
        Persistent = true;
        RandomizedDelaySec = "5m";
      };
    };

    systemd.services.axismundi-backup-offsite = mkIf cfg.backup.offsite.enable {
      description = "axismundi: encrypt + upload latest backup to b2";
      after = [ "axismundi-backup.service" ];
      serviceConfig = {
        Type = "oneshot";
        ExecStart = "${offsiteScript}/bin/axismundi-backup-offsite";
      };
    };

    systemd.timers.axismundi-backup-offsite = mkIf cfg.backup.offsite.enable {
      description = "axismundi: offsite backup timer";
      wantedBy = [ "timers.target" ];
      timerConfig = {
        OnCalendar = cfg.backup.offsite.schedule;
        Persistent = true;
        RandomizedDelaySec = "15m";
      };
    };
  };
}
