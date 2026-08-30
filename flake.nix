{
  description = "Axismundi Rust web application";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      flake-utils,
    }:
    let
      # nixos module: consumed by a system flake to declare the deployment.
      # not per-system; lives outside eachDefaultSystem.
      nixosModule = import ./nix/module.nix;
    in
    {
      nixosModules.default = nixosModule;
      nixosModules.axismundi = nixosModule;
    }
    // flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [
            "rust-src"
            "rust-analyzer"
            "llvm-tools-preview"
          ];
        };

        # buildRustPackage needs a rustPlatform built around our pinned toolchain,
        # not the nixpkgs default rustc.
        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };

        axismundi = pkgs.callPackage ./nix/package.nix {
          inherit rustPlatform;
        };

        nativeBuildInputs = with pkgs; [
          rustToolchain
          pkg-config
          bun
          sqlx-cli
          minio-client
          just
          cargo-llvm-cov
          pstree
          watchexec
          concurrently
          clang
          llvmPackages.libclang
          graphviz
          jq
          age
          rclone
        ];

        buildInputs =
          with pkgs;
          [
            openssl
            postgresql_18
            graphviz
            libwebp
          ]
          ++ lib.optionals stdenv.isDarwin [
            darwin.apple_sdk.frameworks.Security
            darwin.apple_sdk.frameworks.CoreFoundation
          ];

        # convenience helper for the source = "local" deploy loop.
        # backs up prod, dry-runs pending migrations, builds the image, and
        # restarts the systemd unit. assumes the user has sudo for the restart.
        # usage from a clone of this repo: `nix run .#deploy-local`
        deploy-local = pkgs.writeShellApplication {
          name = "axismundi-deploy-local";
          runtimeInputs = with pkgs; [
            podman
            postgresql_18
            sqlx-cli
            jq
            git
            coreutils
          ];
          text = ''
            exec ${pkgs.bash}/bin/bash "$(git rev-parse --show-toplevel)/scripts/deploy.sh" "$@"
          '';
        };

      in
      {
        # mkShell takes its stdenv through .override -- passing `stdenv = ...`
        # in the attrset is silently ignored and you get the plain gcc stdenv.
        devShells.default =
          (pkgs.mkShell.override {
            stdenv = pkgs.stdenvAdapters.useMoldLinker pkgs.clangStdenv;
          })
            {
              inherit nativeBuildInputs buildInputs;
              LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
            };

        packages.default = axismundi;
        packages.axismundi = axismundi;
        packages.deploy-local = deploy-local;
        apps.deploy-local = {
          type = "app";
          program = "${deploy-local}/bin/axismundi-deploy-local";
        };
      }
    );
}
