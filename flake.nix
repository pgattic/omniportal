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

      perSystem = { pkgs, ... }: {
        devShells.default = pkgs.mkShell {
          NIX_LD = pkgs.stdenv.cc.bintools.dynamicLinker;
          NIX_LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
            pkgs.stdenv.cc.cc
            pkgs.zlib
          ];

          packages = with pkgs; [
            espflash
            espup
            ldproxy
            file
            patchelf
            pkg-config
            rustup
            udev
          ];

          shellHook = ''
            export RUSTUP_HOME="''${RUSTUP_HOME:-$PWD/.rustup}"
            export CARGO_HOME="''${CARGO_HOME:-$PWD/.cargo-home}"
            export RUSTUP_TOOLCHAIN="''${RUSTUP_TOOLCHAIN:-esp}"
            export PATH="$CARGO_HOME/bin:$PATH"

            if [ -f "$PWD/export-esp.sh" ]; then
              source "$PWD/export-esp.sh"
            fi

            echo "ESP32-S3 dev shell"
            echo "  first setup: espup install --targets esp32s3 --export-file $PWD/export-esp.sh"
            echo "  on NixOS:     scripts/patch-esp-toolchain-nixos.sh"
            echo "  build:       cargo build"
            echo "  flash:       espflash flash --monitor target/xtensa-esp32s3-none-elf/debug/omniportal"
          '';
        };
      };
    };
}
