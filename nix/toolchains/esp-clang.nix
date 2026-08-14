{
  autoPatchelfHook,
  fetchurl,
  lib,
  stdenv,
  zlib,
}:

let
  version = "20.1.1_20250829";
  host = stdenv.hostPlatform.config;
  archiveHost =
    {
      aarch64-unknown-linux-gnu = "aarch64-linux-gnu";
      x86_64-unknown-linux-gnu = "x86_64-linux-gnu";
    }.${host}
      or (throw "unsupported ESP clang host platform: ${host}");
  hashes = {
    aarch64-unknown-linux-gnu = "sha256-QfU2/e4iUnAR2J5BazAWSK/Z9zjQPhxi8rJ+Nuw5XEw=";
    x86_64-unknown-linux-gnu = "sha256-iJEMITUMBqUh8kMwTRo629t4RHEjs/jidJOqt148wH8=";
  };
in
stdenv.mkDerivation {
  pname = "esp-clang";
  inherit version;

  src = fetchurl {
    url = "https://github.com/espressif/llvm-project/releases/download/esp-${version}/clang-esp-${version}-${archiveHost}.tar.xz";
    hash =
      hashes.${host}
        or (throw "unsupported ESP clang host platform: ${host}");
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
    description = "Espressif LLVM/clang toolchain";
    homepage = "https://github.com/espressif/llvm-project";
    license = lib.licenses.asl20;
    platforms = [
      "aarch64-linux"
      "x86_64-linux"
    ];
  };
}
