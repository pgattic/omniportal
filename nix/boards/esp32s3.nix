{ pkgs }:

let
  espRust = pkgs.callPackage ../toolchains/esp-rust.nix { };
  espClang = pkgs.callPackage ../toolchains/esp-clang.nix { };
  xtensaEspElf = pkgs.callPackage ../toolchains/xtensa-esp-elf.nix { };

  target = "xtensa-esp32s3-none-elf";
  partitionTable = "partitions/esp32s3-n16r8.csv";
  firmwareElf = "target/${target}/release/omniportal";

  commonRuntimeInputs = [
    espRust
    espClang
    xtensaEspElf
    pkgs.espflash
    pkgs.ldproxy
    pkgs.libusb1
    pkgs.pkg-config
    pkgs.stdenv.cc
  ];

  espEnv = ''
    unset RUSTUP_TOOLCHAIN
    unset RUSTUP_HOME
    export CARGO_HOME="''${CARGO_HOME:-$PWD/.cargo-home}"
    export RUSTC="${espRust}/bin/rustc"
    export CARGO_BUILD_TARGET="''${CARGO_BUILD_TARGET:-${target}}"
    export CARGO_UNSTABLE_BUILD_STD="''${CARGO_UNSTABLE_BUILD_STD:-core,alloc}"
    export LIBCLANG_PATH="${espClang}/lib"
    export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath [ pkgs.libusb1 ]}:''${LD_LIBRARY_PATH:-}"
  '';

  firmware-build = pkgs.writeShellApplication {
    name = "omniportal-esp32s3-build";
    runtimeInputs = commonRuntimeInputs;
    text = ''
      ${espEnv}
      exec "${espRust}/bin/cargo" build --release "$@"
    '';
  };

  firmware-flash = pkgs.writeShellApplication {
    name = "omniportal-esp32s3-flash";
    runtimeInputs = commonRuntimeInputs;
    text = ''
      ${espEnv}
      exec "${pkgs.espflash}/bin/espflash" flash \
        --partition-table "${partitionTable}" \
        --monitor \
        "$@" \
        "${firmwareElf}"
    '';
  };
in
{
  inherit espRust espClang xtensaEspElf firmware-build firmware-flash;

  packages = commonRuntimeInputs ++ [
    firmware-build
    firmware-flash
  ];

  shellHook = ''
    ${espEnv}

    echo "ESP32-S3 dev shell"
    echo "  build:      nix run .#esp32s3-build"
    echo "  in shell:   omniportal-esp32s3-build"
    echo "  flash:      nix run .#esp32s3-flash -- /dev/ttyACM0"
    echo "  host tests: omniportal-host-test"
  '';
}
