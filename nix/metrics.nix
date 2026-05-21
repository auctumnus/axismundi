{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.axismundi;
  mcfg = cfg.metrics;
  helpers = import ./lib.nix { inherit lib pkgs; };
  inherit (helpers)
    networkName
    runtimeDir
    secretSubmodule
    mkSecretAssert
    resolveSecret
    ;
  rt = helpers.mkRuntime cfg;
  inherit (rt) isPackageSource;
  inherit (lib)
    mkIf
    mkOption
    mkEnableOption
    types
    optional
    ;

  appScrapeTarget =
    if isPackageSource then
      "host.containers.internal:${toString cfg.config.metricsPort}"
    else
      "axismundi:${toString cfg.config.metricsPort}";

  alloyConfig = pkgs.writeText "axismundi-alloy.config.alloy" ''
    prometheus.scrape "axismundi_app" {
      targets    = [{ "__address__" = "${appScrapeTarget}" }]
      forward_to = [prometheus.remote_write.grafana_cloud.receiver]
      scrape_interval = "30s"
    }

    prometheus.scrape "minio" {
      targets    = [{ "__address__" = "axismundi-minio:9000" }]
      metrics_path = "/minio/v2/metrics/cluster"
      bearer_token = sys.env("MINIO_PROM_TOKEN")
      forward_to = [prometheus.remote_write.grafana_cloud.receiver]
      scrape_interval = "60s"
    }

    prometheus.scrape "cadvisor" {
      targets    = [{ "__address__" = "axismundi-cadvisor:8080" }]
      forward_to = [prometheus.remote_write.grafana_cloud.receiver]
      scrape_interval = "30s"
    }

    prometheus.remote_write "grafana_cloud" {
      endpoint {
        url = sys.env("GRAFANA_CLOUD_PROM_URL")
        basic_auth {
          username = sys.env("GRAFANA_CLOUD_PROM_USER")
          password = sys.env("GRAFANA_CLOUD_PROM_TOKEN")
        }
      }
    }
  '';

  metricsTokens = {
    grafanaCloudToken = resolveSecret "grafana-cloud-token" mcfg.grafanaCloud.token;
    minioToken = resolveSecret "minio-prom-token" mcfg.minioToken;
  };

  generateAlloyEnvScript = pkgs.writeShellApplication {
    name = "axismundi-metrics-generate-env";
    runtimeInputs = [ pkgs.coreutils ];
    text = ''
      set -euo pipefail
      umask 0077
      gc_token=$(cat ${metricsTokens.grafanaCloudToken})
      minio_token=$(cat ${metricsTokens.minioToken})
      {
        printf 'GRAFANA_CLOUD_PROM_URL=%s\n' "${mcfg.grafanaCloud.url}"
        printf 'GRAFANA_CLOUD_PROM_USER=%s\n' "${mcfg.grafanaCloud.username}"
        printf 'GRAFANA_CLOUD_PROM_TOKEN=%s\n' "$gc_token"
        printf 'MINIO_PROM_TOKEN=%s\n' "$minio_token"
      } > ${runtimeDir}/alloy.env
    '';
  };
in
{
  options.services.axismundi = {
    metrics = {
      enable = mkEnableOption "grafana cloud metrics shipping via alloy + cadvisor";

      grafanaCloud = {
        url = mkOption {
          type = types.str;
          example = "https://prometheus-prod-XX.grafana.net/api/prom/push";
          description = "grafana cloud prometheus remote_write endpoint url.";
        };
        username = mkOption {
          type = types.str;
          description = "grafana cloud numeric instance id (basic_auth username).";
        };
        token = secretSubmodule ''
          grafana cloud access policy token (basic_auth password). needs
          `metrics:write` scope.
        '';
      };

      minioToken = secretSubmodule ''
        bearer token for minio /v2/metrics. generate once with
        `mc admin prometheus generate <alias>` and store the resulting
        JWT in a file readable by root.
      '';
    };

    # always-on option (no `metrics.enable` gate) - the rust app serves
    # /metrics on this port whether or not scraping is configured
    config.metricsPort = mkOption {
      type = types.port;
      default = 9091;
      description = ''
        tcp port the app exposes /metrics on. only consumed by the alloy
        scrape target when services.axismundi.metrics.enable = true.
      '';
    };
  };

  config = mkIf (cfg.enable && mcfg.enable) {
    assertions = [
      (mkSecretAssert "metrics.grafanaCloud.token" true mcfg.grafanaCloud.token)
      (mkSecretAssert "metrics.minioToken" true mcfg.minioToken)
      {
        assertion = cfg.minio.enable;
        message = "services.axismundi.metrics requires minio.enable = true (nothing to scrape otherwise)";
      }
    ];

    systemd.services.axismundi-metrics-config = {
      description = "render alloy.env for axismundi metrics shipping";
      wantedBy = [ "multi-user.target" ];
      after = [ "axismundi-config.service" ];
      requires = [ "axismundi-config.service" ];
      before = [ "podman-axismundi-alloy.service" ];
      restartTriggers = [ generateAlloyEnvScript ];
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
        ExecStart = "${generateAlloyEnvScript}/bin/axismundi-metrics-generate-env";
      };
    };

    systemd.services.podman-axismundi-alloy = {
      after = [ "axismundi-metrics-config.service" ];
      requires = [ "axismundi-metrics-config.service" ];
    };

    virtualisation.oci-containers.containers = {
      axismundi-alloy = {
        image = "grafana/alloy:latest";
        autoStart = true;
        cmd = [
          "run"
          "--server.http.listen-addr=0.0.0.0:12345"
          "/etc/alloy/config.alloy"
        ];
        volumes = [ "${alloyConfig}:/etc/alloy/config.alloy:ro" ];
        environmentFiles = [ "${runtimeDir}/alloy.env" ];
        dependsOn =
          [ "axismundi-cadvisor" ]
          ++ optional cfg.minio.enable "axismundi-minio";
        extraOptions = [
          "--network=${networkName}"
          "--log-driver=journald"
          # `host.containers.internal` resolves to the host on the default
          # podman network; on a *custom* network it has to be added by
          # hand. needed in package-source mode where the app runs as a
          # host systemd service, not a container.
          "--add-host=host.containers.internal:host-gateway"
        ];
      };

      axismundi-cadvisor = {
        image = "gcr.io/cadvisor/cadvisor:latest";
        autoStart = true;
        # cadvisor needs broad read access to host paths to discover
        # containers and read their cgroup stats. mounts are read-only.
        volumes = [
          "/:/rootfs:ro"
          "/var/run:/var/run:ro"
          "/sys:/sys:ro"
          "/var/lib/containers/storage:/var/lib/containers/storage:ro"
        ];
        extraOptions = [
          "--network=${networkName}"
          "--log-driver=journald"
          "--privileged"
          "--device=/dev/kmsg"
        ];
      };
    };
  };
}
