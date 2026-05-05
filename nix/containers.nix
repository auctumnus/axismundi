{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.axismundi;
  helpers = import ./lib.nix { inherit lib pkgs; };
  inherit (helpers) networkName runtimeDir;
  rt = helpers.mkRuntime cfg;
  inherit (rt)
    isLocalSource
    isPackageSource
    appImage
    postgresHostPort
    minioHostPort
    lexurgyHostPort
    ;
  inherit (lib) mkIf optional optionalAttrs;
in
{
  config = mkIf cfg.enable {
    virtualisation.podman = {
      enable = true;
      defaultNetwork.settings.dns_enabled = true;
    };

    systemd.tmpfiles.rules = [
      "d ${cfg.stateDir} 0750 root root - -"
      "d ${cfg.stateDir}/postgres 0700 999 999 - -"
      "d ${cfg.stateDir}/minio 0750 root root - -"
      "d ${cfg.backupDir} 0750 root root - -"
    ];

    systemd.services.axismundi-network = {
      description = "create the axismundi podman network";
      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" ];
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
      };
      script = ''
        ${pkgs.podman}/bin/podman network exists ${networkName} \
          || ${pkgs.podman}/bin/podman network create ${networkName}
      '';
    };

    # source = "local" first-boot UX: without this, podman-axismundi.service
    # crash-loops with "image not known" until it hits start-limit-hit and
    # gives up. that looks like a hard failure even though all the user
    # needs to do is run `just deploy` (or `podman build`). instead, this
    # oneshot blocks on the image existing — the consuming unit stays in
    # "activating" with a clear journal trail until the image is present.
    systemd.services.axismundi-image-wait = mkIf isLocalSource {
      description = "wait for axismundi:local image to be present";
      before = [ "podman-axismundi.service" ];
      after = [ "axismundi-network.service" ];
      requires = [ "axismundi-network.service" ];
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
        # the image might genuinely never appear (user forgets to run
        # `just deploy`). don't time out — let the user see the journal
        # message and act, rather than failing the unit.
        TimeoutStartSec = "infinity";
      };
      script = ''
        attempt=0
        until ${pkgs.podman}/bin/podman image exists axismundi:local; do
          if [ "$attempt" -eq 0 ]; then
            echo "axismundi:local image not present — waiting." >&2
            echo "build it: cd /opt/axismundi && podman build -t axismundi:local ." >&2
            echo "or:      cd /opt/axismundi && just deploy" >&2
          elif [ "$((attempt % 30))" -eq 0 ]; then
            echo "still waiting for axismundi:local image (~$((attempt / 6)) min elapsed)" >&2
          fi
          attempt=$((attempt + 1))
          sleep 10
        done
        echo "axismundi:local image is present" >&2
      '';
    };

    # gate the auto-generated podman-axismundi unit on the image-wait so
    # systemd doesn't burn its 5-restart budget while the image is missing.
    systemd.services.podman-axismundi = mkIf isLocalSource {
      after = [ "axismundi-image-wait.service" ];
      requires = [ "axismundi-image-wait.service" ];
    };

    virtualisation.oci-containers = {
      backend = "podman";
      containers =
        let
          common = [
            "--network=${networkName}"
            "--log-driver=journald"
          ];
        in
        optionalAttrs (!isPackageSource) {
          axismundi = {
            image = appImage;
            autoStart = true;
            environmentFiles = optional (cfg.envFile != null) cfg.envFile;
            
            ports = [ "127.0.0.1:${toString cfg.config.port}:${toString cfg.config.port}" ];
            volumes = [ "${runtimeDir}/config.json:/app/config.json:ro" ];
            dependsOn = optional cfg.postgres.enable "axismundi-postgres";
            extraOptions =
              common
              ++ [ "--restart=on-failure" ]
              ++ optional isLocalSource "--pull=never";
          };
        }
        // optionalAttrs cfg.postgres.enable {
          axismundi-postgres = {
            image = "postgres:18";
            autoStart = true;
            environment = {
              POSTGRES_DB = "axismundi";
              POSTGRES_USER = "axismundi";
            };
            environmentFiles = [ "${runtimeDir}/postgres.env" ];
            volumes = [ "${cfg.stateDir}/postgres:/var/lib/postgresql" ];
            ports = optional (postgresHostPort != null) "127.0.0.1:${toString postgresHostPort}:5432";
            extraOptions = common ++ [
              "--health-cmd=pg_isready -U axismundi -d axismundi"
              "--health-interval=10s"
              "--health-timeout=5s"
              "--health-retries=5"
            ];
          };
        }
        // optionalAttrs cfg.minio.enable {
          axismundi-minio = {
            image = "minio/minio:latest";
            autoStart = true;
            cmd = [
              "server"
              "/data"
              "--console-address"
              ":9001"
            ];
            environmentFiles = [ "${runtimeDir}/minio.env" ];
            volumes = [ "${cfg.stateDir}/minio:/data" ];
            ports = optional (minioHostPort != null) "127.0.0.1:${toString minioHostPort}:9000";
            extraOptions = common ++ [
              "--health-cmd=curl -f http://localhost:9000/minio/health/live"
              "--health-interval=30s"
            ];
          };
        }
        // optionalAttrs cfg.imagor.enable {
          axismundi-imagor = {
            image = "shumc/imagor:latest";
            autoStart = true;
            environment = {
              PORT = "8000";
              S3_ENDPOINT = "http://axismundi-minio:9000";
              S3_FORCE_PATH_STYLE = "1";
              # imagor's s3 loader uses aws-sdk-go-v2, which refuses to
              # initialize without a region. when that init fails imagor
              # silently falls through to the http loader, which tries to
              # fetch `originals/...` as `https://originals/...` and 404s.
              AWS_REGION = cfg.config.s3.region;
              S3_LOADER_BUCKET = cfg.minio.bucket;
              S3_RESULT_STORAGE_BUCKET = cfg.minio.bucket;
              S3_RESULT_STORAGE_BASE_DIR = "results";
              IMAGOR_AUTO_WEBP = "1";
            };
            environmentFiles = [ "${runtimeDir}/imagor.env" ];
            ports = [ "127.0.0.1:${toString cfg.imagor.port}:8000" ];
            dependsOn = optional cfg.minio.enable "axismundi-minio";
            extraOptions = common;
          };
        }
        // optionalAttrs cfg.lexurgy.enable {
          axismundi-lexurgy = {
            image = "ghcr.io/auctumnus/lexurgy-services:latest";
            autoStart = true;
            environment = {
              SINGLE_STEP_TIMEOUT = "1";
              REQUEST_TIMEOUT = "5";
              TOTAL_TIMEOUT = "60";
            };
            environmentFiles = [ "${runtimeDir}/lexurgy.env" ];
            ports = optional (lexurgyHostPort != null) "127.0.0.1:${toString lexurgyHostPort}:8080";
            extraOptions = common;
          };
        };
    };

    systemd.services.axismundi-minio-init = mkIf cfg.minio.enable {
      description = "create axismundi minio bucket if it doesn't exist";
      after = [
        "podman-axismundi-minio.service"
        "axismundi-config.service"
      ];
      wants = [ "podman-axismundi-minio.service" ];
      wantedBy = [ "multi-user.target" ];
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
      };
      script = ''
        for i in $(seq 1 30); do
          if ${pkgs.podman}/bin/podman run --rm --network=${networkName} \
              --env-file=${runtimeDir}/minio.env \
              minio/mc:latest \
              alias set local http://axismundi-minio:9000 \
                "$MINIO_ROOT_USER" "$MINIO_ROOT_PASSWORD" >/dev/null 2>&1; then
            break
          fi
          sleep 2
        done

        # the minio/mc:latest image has `mc` as its entrypoint, so passing
        # `sh -c …` after the image ref ends up running `mc sh -c …` and
        # dying with "sh is not a recognized command". clear the entrypoint
        # so the cmd is interpreted as a literal shell invocation.
        ${pkgs.podman}/bin/podman run --rm --network=${networkName} \
          --entrypoint="" \
          --env-file=${runtimeDir}/minio.env \
          minio/mc:latest sh -c '
            mc alias set local http://axismundi-minio:9000 "$MINIO_ROOT_USER" "$MINIO_ROOT_PASSWORD" &&
            mc mb local/${cfg.minio.bucket} --ignore-existing &&
            mc anonymous set download local/${cfg.minio.bucket}
          '
      '';
    };
  };
}
