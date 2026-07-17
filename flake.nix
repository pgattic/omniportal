{
  description = "OmniPortal ESP32-S3 firmware development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
  };

  outputs = inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      perSystem = { pkgs, ... }:
        let
          omniportal-host-test = pkgs.writeShellApplication {
            name = "omniportal-host-test";
            runtimeInputs = [
              pkgs.cargo
              pkgs.rustc
            ];
            text = ''
              unset RUSTUP_TOOLCHAIN
              unset CARGO_BUILD_TARGET
              unset CARGO_UNSTABLE_BUILD_STD
              export RUSTC="${pkgs.rustc}/bin/rustc"
              exec "${pkgs.cargo}/bin/cargo" test --lib "$@"
            '';
          };
        in
      {
        devShells.default = pkgs.mkShell {
          NIX_LD = pkgs.stdenv.cc.bintools.dynamicLinker;
          NIX_LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
            pkgs.stdenv.cc.cc
            pkgs.libusb1
            pkgs.zlib
          ];

          packages = with pkgs; [
            espflash
            espup
            ldproxy
            file
            omniportal-host-test
            libusb1
            patchelf
            pkg-config
            (python3.withPackages (python-pkgs: [
              python-pkgs.pyusb
            ]))
            rustup
            udev
          ];

          shellHook = ''
            export RUSTUP_HOME="''${RUSTUP_HOME:-$PWD/.rustup}"
            export CARGO_HOME="''${CARGO_HOME:-$PWD/.cargo-home}"
            export RUSTUP_TOOLCHAIN="''${RUSTUP_TOOLCHAIN:-esp}"
            export CARGO_BUILD_TARGET="''${CARGO_BUILD_TARGET:-xtensa-esp32s3-none-elf}"
            export CARGO_UNSTABLE_BUILD_STD="''${CARGO_UNSTABLE_BUILD_STD:-core,alloc}"
            export PATH="$CARGO_HOME/bin:$PATH"
            export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath [ pkgs.libusb1 ]}:''${LD_LIBRARY_PATH:-}"

            if [ -f "$PWD/export-esp.sh" ]; then
              source "$PWD/export-esp.sh"
            fi

            echo "ESP32-S3 dev shell"
            echo "  first setup: espup install --targets esp32s3 --export-file $PWD/export-esp.sh"
            echo "  on NixOS:     scripts/patch-esp-toolchain-nixos.sh"
            echo "  build:       cargo build"
            echo "  host tests:  omniportal-host-test"
            echo "  flash:       espflash flash --partition-table partitions/esp32s3-n16r8.csv --monitor target/xtensa-esp32s3-none-elf/debug/omniportal"
          '';
        };
      };
    };
}
