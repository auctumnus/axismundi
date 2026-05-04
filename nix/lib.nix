{ lib, pkgs }:
let
  inherit (lib) mkOption types;
in
rec {
  networkName = "axismundi";
  # NOT /run/axismundi — that collides with the podman-axismundi.service's
  # auto-generated RuntimeDirectory=axismundi (derived from the container
  # name). systemd manages those independently, and the podman service
  # has RuntimeDirectoryPreserve=no, so every time the app container
  # stops (e.g. during `deploy.sh`) it would wipe config.json and the
  # env files axismundi-config wrote into the same directory.
  runtimeDir = "/run/axismundi-runtime";
  appUser = "axismundi";
  appGroup = "axismundi";
  pgPackage = pkgs.postgresql_18;
  scriptsSrc = ../scripts;

  default-postgres-port = 5432;
  default-minio-port = 9000;
  default-lexurgy-port = 8080;

  resolveSecret =
    name: secret:
    if secret.file != null then
      "${secret.file}"
    else if secret.value != null then
      "${pkgs.writeText "axismundi-${name}" secret.value}"
    else
      null;

  secretSubmodule =
    description:
    mkOption {
      inherit description;
      default = { };
      type = types.submodule {
        options = {
          value = mkOption {
            type = types.nullOr types.str;
            default = null;
            description = ''
              plaintext secret. WILL BE COPIED INTO THE NIX STORE, which is
              world-readable on most systems. only use when you accept that
              tradeoff (single-user host where leakage isn't a concern).
              prefer .file for production / multi-user hosts.
            '';
          };
          file = mkOption {
            type = types.nullOr types.path;
            default = null;
            description = ''
              path to a file containing the secret. read at activation;
              never enters the nix store. compatible with agenix /
              sops-nix / manually-placed files.
            '';
          };
        };
      };
    };

  hasOnlyOne = s: !((s.value != null) && (s.file != null));
  hasExactlyOne = s: (s.value != null) != (s.file != null);
  mkSecretAssert = name: required: s: {
    assertion = if required then hasExactlyOne s else hasOnlyOne s;
    message =
      if required then
        "services.axismundi.${name}: must set exactly one of .value or .file"
      else
        "services.axismundi.${name}: at most one of .value or .file may be set";
  };

  mkRuntime =
    cfg:
    let
      isLocalSource = cfg.source == "local";
      isPackageSource = !isLocalSource && cfg.source.package != null;
      isOciSource = !isLocalSource && cfg.source.package == null;
    in
    {
      inherit isLocalSource isPackageSource isOciSource;
      appImage =
        if isLocalSource then
          "axismundi:local"
        else if isOciSource then
          "${cfg.source.registry}:${cfg.source.tag}"
        else
          null;
      appPackage = if isPackageSource then cfg.source.package else null;
      postgresHostPort =
        if cfg.postgres.hostPort != null then
          cfg.postgres.hostPort
        else if isPackageSource then
          default-postgres-port
        else
          null;
      minioHostPort =
        if cfg.minio.hostPort != null then
          cfg.minio.hostPort
        else if isPackageSource then
          default-minio-port
        else
          null;
      lexurgyHostPort =
        if cfg.lexurgy.hostPort != null then
          cfg.lexurgy.hostPort
        else if isPackageSource then
          default-lexurgy-port
        else
          null;
    };
}
