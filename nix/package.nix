{
  lib,
  stdenv,
  rustPlatform,
  nodejs_20,
  pnpm,
  fetchPnpmDeps,
  pnpmConfigHook,
  pkg-config,
  xz,
  src,
}:

let
  frontendSrc = "${src}/frontend";
  backendSrc = "${src}/backend";

  # Build the frontend first as a separate derivation
  frontend = stdenv.mkDerivation {
    pname = "anyserver-frontend";
    version = "0.2.3";

    src = frontendSrc;

    nativeBuildInputs = [
      nodejs_20
      pnpm
      pnpmConfigHook
    ];

    pnpmDeps = fetchPnpmDeps {
      pname = "anyserver-frontend";
      src = frontendSrc;
      hash = "sha256-6UMtHBR5/UgjkjALndUO8u2oyJJQ9T5G8dnoRjzoSG0=";
      fetcherVersion = 3;
    };

    buildPhase = ''
      runHook preBuild
      pnpm run build
      runHook postBuild
    '';

    installPhase = ''
      runHook preInstall
      cp -r dist $out
      runHook postInstall
    '';
  };
in

rustPlatform.buildRustPackage {
  pname = "anyserver";
  version = "0.2.3";

  src = backendSrc;

  cargoHash = "sha256-U4D9mDOKTQdFTDvaBQJiHcfWETCOFbhjtI42/eItkhA=";

  nativeBuildInputs = [
    pkg-config
  ];

  buildInputs = [
    xz
  ];

  buildFeatures = [ "bundle-frontend" ];

  # Copy the pre-built frontend dist into the expected location so
  # rust-embed can find it during the backend build.
  preBuild = ''
    mkdir -p ../frontend
    cp -r ${frontend} ../frontend/dist
    export SKIP_FRONTEND_BUILD=1
  '';

  # SQLx offline mode is already configured via backend/.cargo/config.toml
  # (SQLX_OFFLINE = "true"), and the .sqlx/ query cache is in the repo.

  # Disable tests — they require a running database / network access
  doCheck = false;

  meta = with lib; {
    description = "A self-hosted panel for running any binary as a managed server";
    longDescription = ''
      AnyServer is a self-hosted panel for running any binary as a managed
      server. It provides auto-start, auto-restart, a live console, file
      management, templates, and more — all from a single binary with zero
      external dependencies.
    '';
    homepage = "https://github.com/NexRX/AnyServer";
    license = licenses.agpl3Plus;
    maintainers = [ ];
    platforms = platforms.linux;
    mainProgram = "anyserver";
  };
}
