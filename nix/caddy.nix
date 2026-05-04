{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.axismundi;
  inherit (lib) mkIf mkDefault optionalAttrs;

  fallbackRoot = pkgs.linkFarm "axismundi-fallback" [
    {
      name = "down.html";
      path = cfg.caddy.fallbackPage;
    }
  ];
in
{
  config = mkIf (cfg.enable && cfg.caddy.enable) {
    services.caddy = {
      enable = mkDefault true;
      virtualHosts =
        {
          ${cfg.caddy.domain} = {
            extraConfig = ''
              reverse_proxy 127.0.0.1:${toString cfg.config.port} 127.0.0.1:${toString cfg.caddy.fallbackPort} {
                lb_policy first
                lb_try_duration 1s
                fail_duration 30s
                transport http {
                  dial_timeout 500ms
                }
              }

              ${cfg.caddy.extraConfig}
            '';
          };
          "http://127.0.0.1:${toString cfg.caddy.fallbackPort}" = {
            extraConfig = ''
              rewrite * /down.html
              root * ${fallbackRoot}
              file_server
            '';
          };
        }
        // optionalAttrs (cfg.caddy.mediaDomain != null) {
          ${cfg.caddy.mediaDomain} = {
            extraConfig = ''
              reverse_proxy 127.0.0.1:${toString cfg.imagor.port}
            '';
          };
        };
    };
  };
}
