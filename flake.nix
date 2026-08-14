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
          esp32s3 = import ./nix/boards/esp32s3.nix { inherit pkgs; };

          omniportal-host-test = pkgs.writeShellApplication {
            name = "omniportal-host-test";
            runtimeInputs = [
              pkgs.cargo
              pkgs.rustc
              pkgs.stdenv.cc
            ];
            text = ''
              unset RUSTUP_TOOLCHAIN
              unset CARGO_BUILD_TARGET
              unset CARGO_UNSTABLE_BUILD_STD
              export RUSTC="${pkgs.rustc}/bin/rustc"
              exec "${pkgs.cargo}/bin/cargo" test --target "${pkgs.stdenv.hostPlatform.config}" --lib "$@"
            '';
          };
        in
      {
        packages = {
          esp-rust = esp32s3.espRust;
          esp-clang = esp32s3.espClang;
          xtensa-esp-elf = esp32s3.xtensaEspElf;
        };

        apps = {
          esp32s3-build = {
            type = "app";
            program = "${esp32s3.firmware-build}/bin/omniportal-esp32s3-build";
          };

          esp32s3-flash = {
            type = "app";
            program = "${esp32s3.firmware-flash}/bin/omniportal-esp32s3-flash";
          };

          host-test = {
            type = "app";
            program = "${omniportal-host-test}/bin/omniportal-host-test";
          };
        };

        devShells.default = pkgs.mkShell {
          packages = esp32s3.packages ++ (with pkgs; [
            omniportal-host-test
            (python3.withPackages (python-pkgs: [
              python-pkgs.pyusb
            ]))
            udev
          ]);

          shellHook = esp32s3.shellHook;
        };
      };
    };
}
