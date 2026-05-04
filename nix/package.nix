{
  lib,
  stdenv,
  rustPlatform,
  pkg-config,
  makeWrapper,
  bun,
  nodejs,
  openssl,
  libwebp,
  graphviz,
  llvmPackages,
}:

let
  # filter the repo source: keep everything the rust + frontend builds touch,
  # drop build outputs, vcs metadata, and runtime-only files.
  srcFilter =
    path: type:
    let
      name = baseNameOf path;
      relPath = lib.removePrefix (toString ../. + "/") (toString path);
    in
    !(builtins.elem name [
      "target"
      "node_modules"
      ".git"
      ".direnv"
      ".parcel-cache"
      "backups"
      "lcov.info"
      "config.json"
      "session.json"
    ])
    && !(lib.hasPrefix ".env" name)
    && relPath != "frontend/dist";

  src = lib.cleanSourceWith {
    src = ../.;
    filter = srcFilter;
    name = "axismundi-source";
  };

  frontendSrc = lib.cleanSourceWith {
    src = ../frontend;
    filter =
      path: type:
      let
        name = baseNameOf path;
      in
      !(builtins.elem name [
        "node_modules"
        "dist"
        ".parcel-cache"
      ]);
    name = "axismundi-frontend-source";
  };

  # FOD: bun install pulls from npm, so we have to declare the hash and let
  # nix punch a hole in the sandbox for it. rotate via lib.fakeHash whenever
  # bun.lock changes.
  frontendDeps = stdenv.mkDerivation {
    pname = "axismundi-frontend-deps";
    version = "0.1.0";
    src = frontendSrc;

    nativeBuildInputs = [ bun ];

    dontConfigure = true;
    dontFixup = true;

    buildPhase = ''
      runHook preBuild
      export HOME=$TMPDIR
      bun install \
        --frozen-lockfile \
        --no-progress \
        --ignore-scripts
      runHook postBuild
    '';

    installPhase = ''
      runHook preInstall
      mkdir -p $out
      cp -R node_modules $out/
      runHook postInstall
    '';

    outputHash = "sha256-DtuV9iX5xyCg265N86fYgfpoCs7iYoDhSIgGqQHbBXc=";
    outputHashAlgo = "sha256";
    outputHashMode = "recursive";
  };

  frontend = stdenv.mkDerivation {
    pname = "axismundi-frontend";
    version = "0.1.0";
    src = frontendSrc;

    # nodejs is here just so node-based shebangs in node_modules/.bin work;
    # the actual builder is bun.
    nativeBuildInputs = [ bun nodejs ];

    dontConfigure = true;
    dontFixup = true;

    buildPhase = ''
      runHook preBuild
      export HOME=$TMPDIR
      cp -R ${frontendDeps}/node_modules .
      chmod -R u+w node_modules
      # .bin entries are symlinks; patch the actual files they point at.
      patchShebangs node_modules
      # generated from lexurgy.grammar; not tracked in git, so produce it here.
      bun run build:lexurgy-grammar
      bun run build
      runHook postBuild
    '';

    installPhase = ''
      runHook preInstall
      # bun build doesn't fail the process on resolve errors, so guard explicitly.
      if [ -z "$(ls -A dist 2>/dev/null)" ]; then
        echo "frontend build produced an empty dist/" >&2
        exit 1
      fi
      mkdir -p $out
      cp -R dist $out/
      runHook postInstall
    '';
  };
in
rustPlatform.buildRustPackage {
  pname = "axismundi";
  version = "0.1.0";

  inherit src;

  cargoLock = {
    lockFile = ../Cargo.lock;
  };

  nativeBuildInputs = [
    pkg-config
    makeWrapper
    llvmPackages.clang
  ];

  buildInputs = [
    openssl
    libwebp
    graphviz
  ];

  # use the checked-in .sqlx cache instead of hitting a real db at compile time.
  SQLX_OFFLINE = "true";
  LIBCLANG_PATH = "${llvmPackages.libclang.lib}/lib";

  # tests need a live postgres; `just test` is the right entry point for those.
  doCheck = false;

  postInstall = ''
    mkdir -p $out/share/axismundi/frontend
    cp -R templates $out/share/axismundi/
    cp -R assets $out/share/axismundi/
    cp -R migrations $out/share/axismundi/
    cp -R ${frontend}/dist $out/share/axismundi/frontend/dist

    # the binary uses ServeDir with relative paths ("assets", "frontend/dist"),
    # so the wrapper has to enter the share dir before exec.
    for bin in axismundi seed; do
      wrapProgram $out/bin/$bin --chdir $out/share/axismundi
    done
  '';

  meta = {
    description = "axismundi web app";
    mainProgram = "axismundi";
    platforms = lib.platforms.linux;
  };
}
