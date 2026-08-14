{
  autoPatchelfHook,
  fetchurl,
  lib,
  stdenv,
  zlib,
}:

let
  version = "15.2.0_20250929";
  host = stdenv.hostPlatform.config;
  archiveHost =
    {
      aarch64-unknown-linux-gnu = "aarch64-linux-gnu";
      x86_64-unknown-linux-gnu = "x86_64-linux-gnu";
    }.${host}
      or (throw "unsupported Xtensa ESP host platform: ${host}");
  hashes = {
    aarch64-unknown-linux-gnu = "sha256-K3m3uL81PkLdHAqh1/BtxoGyx2ltPYLozOc8eND8UXw=";
    x86_64-unknown-linux-gnu = "sha256-F4N34kk0bCMjI8uU8FLN0Lg/zVgdsuIpfmKeA0JD7c8=";
  };
in
stdenv.mkDerivation {
  pname = "xtensa-esp-elf";
  inherit version;

  src = fetchurl {
    url = "https://github.com/espressif/crosstool-NG/releases/download/esp-${version}/xtensa-esp-elf-${version}-${archiveHost}.tar.xz";
    hash =
      hashes.${host}
        or (throw "unsupported Xtensa ESP host platform: ${host}");
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
    cp -r ./* "$out"/

    runHook postInstall
  '';

  meta = {
    description = "Espressif Xtensa GCC toolchain";
    homepage = "https://github.com/espressif/crosstool-NG";
    license = lib.licenses.gpl3Plus;
    platforms = [
      "aarch64-linux"
      "x86_64-linux"
    ];
  };
}
