{
  autoPatchelfHook,
  fetchurl,
  lib,
  stdenv,
  zlib,
}:

let
  version = "1.95.0.0";
  host = stdenv.hostPlatform.config;
  hashes = {
    aarch64-unknown-linux-gnu = "0k44sav9q6kpvhiq29vc60x8cr8cvyqzq1zk92h7g6szh3k8hz8c";
    x86_64-unknown-linux-gnu = "0cwld65c2rmba2bx40d577pidd70dw9jy00zqihsvdpap8jgplma";
  };
  rustSrc = fetchurl {
    url = "https://github.com/esp-rs/rust-build/releases/download/v${version}/rust-src-${version}.tar.xz";
    sha256 = "05vp68gs8h6q4mwrbd5w5w2imjgypx3k1y8shq71rm62g8ryx2vh";
  };
in
stdenv.mkDerivation {
  pname = "esp-rust";
  inherit version;

  src = fetchurl {
    url = "https://github.com/esp-rs/rust-build/releases/download/v${version}/rust-${version}-${host}.tar.xz";
    sha256 =
      hashes.${host}
        or (throw "unsupported ESP Rust host platform: ${host}");
  };

  nativeBuildInputs = [ autoPatchelfHook ];
  buildInputs = [
    stdenv.cc.cc.lib
    zlib
  ];

  dontConfigure = true;
  dontBuild = true;

  installPhase = ''
    runHook preInstall

    mkdir -p "$out"
    for component in cargo clippy-preview rustc rust-std-${host} rustfmt-preview; do
      cp -r "$component"/* "$out"/
    done

    tar -xf "${rustSrc}"
    cp -r rust-src-nightly/rust-src/* "$out"/

    runHook postInstall
  '';

  meta = {
    description = "Espressif Rust fork with Xtensa target support";
    homepage = "https://github.com/esp-rs/rust-build";
    license = with lib.licenses; [ asl20 mit ];
    platforms = [
      "aarch64-linux"
      "x86_64-linux"
    ];
  };
}
