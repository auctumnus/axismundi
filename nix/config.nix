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
    resolveSecret
    ;
  rt = helpers.mkRuntime cfg;
  inherit (rt) isPackageSource postgresHostPort;

  inherit (lib)
    optional
    optionalString
    optionalAttrs
    concatMapStringsSep
    mkIf
    ;

  secrets = {
    postgresPassword =
      if cfg.postgres.enable then
        resolveSecret "postgres-password" cfg.postgres.password
      else
        null;
    externalDatabaseUrl =
      if !cfg.postgres.enable then resolveSecret "database-url" cfg.postgres.databaseUrl else null;
    s3AccessKey = resolveSecret "s3-access-key" cfg.config.s3.accessKey;
    s3SecretKey = resolveSecret "s3-secret-key" cfg.config.s3.secretKey;
    imagorSecret = resolveSecret "imagor-secret" cfg.config.s3.imagorSecret;
    lexurgyApiKey = resolveSecret "lexurgy-api-key" cfg.config.lexurgy.apiKey;
    resendApiKey =
      if cfg.config.email != "mock" then
        resolveSecret "resend-api-key" cfg.config.email.resend.apiKey
      else
        null;
  };

  # placeholder will be replaced with actual secrets at activation
  PLACEHOLDER = "__AXISMUNDI_PLACEHOLDER__";

  configBase = {
    port = cfg.config.port;
    public_url_base = cfg.config.publicUrlBase;
    environment = cfg.config.environment;
    file_upload_limit_bytes = cfg.config.fileUploadLimitBytes;
    s3 =
      {
        bucket = cfg.config.s3.bucket;
        region = cfg.config.s3.region;
        endpoint = cfg.config.s3.endpoint;
        access_key = PLACEHOLDER;
        secret_key = PLACEHOLDER;
      }
      // optionalAttrs (cfg.config.s3.publicUrlBase != null) {
        public_url_base = cfg.config.s3.publicUrlBase;
      }
      // optionalAttrs (secrets.imagorSecret != null) {
        imagor_secret = PLACEHOLDER;
      };
    lexurgy = {
      url = cfg.config.lexurgy.url;
      api_key = PLACEHOLDER;
    };
    email =
      if cfg.config.email == "mock" then
        "mock"
      else
        {
          resend = {
            from_email = cfg.config.email.resend.fromEmail;
            api_key = PLACEHOLDER;
          };
        };
    database_url = PLACEHOLDER;
    banner = {
      enabled = cfg.config.banner.enabled;
      message = cfg.config.banner.message;
      kind = cfg.config.banner.kind;
    };
    maid = {
      port = cfg.config.maid.port;
      health_check_timeout_ms = cfg.config.maid.healthCheckTimeoutMs;
      wait_between_tasks_ms = cfg.config.maid.waitBetweenTasksMs;
      task_timeout_ms = cfg.config.maid.taskTimeoutMs;
    };
  };

  configTemplate = pkgs.writeText "axismundi-config-template.json" (builtins.toJSON configBase);

  spliceSpecs =
    [
      {
        jqVar = "db_url";
        jqPath = ".database_url";
        bashVar = "database_url";
      }
      {
        jqVar = "s3_access";
        jqPath = ".s3.access_key";
        bashVar = "s3_access";
      }
      {
        jqVar = "s3_secret";
        jqPath = ".s3.secret_key";
        bashVar = "s3_secret";
      }
      {
        jqVar = "lexurgy_key";
        jqPath = ".lexurgy.api_key";
        bashVar = "lexurgy_key";
      }
    ]
    ++ optional (secrets.imagorSecret != null) {
      jqVar = "imagor_secret";
      jqPath = ".s3.imagor_secret";
      bashVar = "imagor_secret";
    }
    ++ optional (cfg.config.email != "mock") {
      jqVar = "resend_key";
      jqPath = ".email.resend.api_key";
      bashVar = "resend_key";
    };

  jqArgFlags = concatMapStringsSep " " (s: ''--arg ${s.jqVar} "''$${s.bashVar}"'') spliceSpecs;
  jqFilter = concatMapStringsSep " | " (s: "(${s.jqPath} = \$${s.jqVar})") spliceSpecs;

  generateConfigScript = pkgs.writeShellApplication {
    name = "axismundi-generate-config";
    runtimeInputs = [
      pkgs.jq
      pkgs.coreutils
    ];
    text = ''
      set -euo pipefail
      install -d -m 0750 ${runtimeDir}
      umask 0077

      ${optionalString cfg.postgres.enable ''
        postgres_password=$(cat ${secrets.postgresPassword})
      ''}
      s3_access=$(cat ${secrets.s3AccessKey})
      s3_secret=$(cat ${secrets.s3SecretKey})
      lexurgy_key=$(cat ${secrets.lexurgyApiKey})
      ${optionalString (secrets.imagorSecret != null) ''
        imagor_secret=$(cat ${secrets.imagorSecret})
      ''}
      ${optionalString (secrets.resendApiKey != null) ''
        resend_key=$(cat ${secrets.resendApiKey})
      ''}

      ${
        if cfg.postgres.enable then
          let
            dbHost = if isPackageSource then "127.0.0.1" else "axismundi-postgres";
            dbPort = if isPackageSource then toString postgresHostPort else "5432";
          in
          ''
            pw_encoded=$(jq -nr --arg p "$postgres_password" '$p | @uri')
            database_url="postgres://axismundi:$pw_encoded@${dbHost}:${dbPort}/axismundi"
          ''
        else
          ''
            database_url=$(cat ${secrets.externalDatabaseUrl})
          ''
      }

      jq ${jqArgFlags} '${jqFilter}' ${configTemplate} > ${runtimeDir}/config.json

      ${optionalString cfg.postgres.enable ''
        printf 'POSTGRES_PASSWORD=%s\n' "$postgres_password" > ${runtimeDir}/postgres.env
      ''}

      ${optionalString cfg.minio.enable ''
        {
          printf 'MINIO_ROOT_USER=%s\n' "$s3_access"
          printf 'MINIO_ROOT_PASSWORD=%s\n' "$s3_secret"
        } > ${runtimeDir}/minio.env
      ''}

      ${optionalString cfg.imagor.enable ''
        {
          printf 'AWS_ACCESS_KEY_ID=%s\n' "$s3_access"
          printf 'AWS_SECRET_ACCESS_KEY=%s\n' "$s3_secret"
          ${
            if (secrets.imagorSecret != null) then
              ''printf 'IMAGOR_SECRET=%s\n' "$imagor_secret"''
            else
              ''printf 'IMAGOR_UNSAFE=1\n' ''
          }
        } > ${runtimeDir}/imagor.env
      ''}

      ${optionalString cfg.lexurgy.enable ''
        printf 'API_KEY=%s\n' "$lexurgy_key" > ${runtimeDir}/lexurgy.env
      ''}

      ${optionalString isPackageSource ''
        # host axismundi.service runs as ${appUser}; let it read the
        # config files we just rendered. dir stays 0750.
        chown -R root:${appGroup} ${runtimeDir}
        chmod -R g+rX ${runtimeDir}
      ''}
    '';
  };
in
{
  config = mkIf cfg.enable {
    systemd.services.axismundi-config = {
      description = "render axismundi config.json and container envfiles";
      wantedBy = [ "multi-user.target" ];
      before =
        optional (!isPackageSource) "podman-axismundi.service"
        ++ optional isPackageSource "axismundi.service"
        ++ optional cfg.postgres.enable "podman-axismundi-postgres.service"
        ++ optional cfg.minio.enable "podman-axismundi-minio.service"
        ++ optional cfg.imagor.enable "podman-axismundi-imagor.service"
        ++ optional cfg.lexurgy.enable "podman-axismundi-lexurgy.service";
      restartTriggers = [
        configTemplate
        generateConfigScript
      ];
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
        RuntimeDirectory = "axismundi";
        RuntimeDirectoryMode = "0750";
        RuntimeDirectoryPreserve = "yes";
        ExecStart = "${generateConfigScript}/bin/axismundi-generate-config";
      };
    };
  };
}
