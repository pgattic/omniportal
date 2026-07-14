#!/usr/bin/env bash
set -euo pipefail

if [[ ! -d .rustup/toolchains/esp ]]; then
  echo "missing .rustup/toolchains/esp; run espup install first" >&2
  exit 1
fi

if [[ -z "${NIX_CC:-}" || ! -f "$NIX_CC/nix-support/dynamic-linker" ]]; then
  echo "run this script from nix develop" >&2
  exit 1
fi

interp="$(<"$NIX_CC/nix-support/dynamic-linker")"
nix_rpath="${NIX_LD_LIBRARY_PATH:-}"

find .rustup/toolchains/esp -type f -print0 |
  while IFS= read -r -d '' file_path; do
    if ! file "$file_path" | grep -q ELF; then
      continue
    fi

    rpath="\$ORIGIN:\$ORIGIN/../lib:\$ORIGIN/../../lib:\$ORIGIN/../../../lib:\$ORIGIN/../../../../lib:\$ORIGIN/rustlib/aarch64-unknown-linux-gnu/lib:\$ORIGIN/../rustlib/aarch64-unknown-linux-gnu/lib:$nix_rpath"

    if patchelf --print-interpreter "$file_path" >/dev/null 2>&1; then
      patchelf --set-interpreter "$interp" "$file_path"
    fi

    if patchelf --set-rpath "$rpath" "$file_path" >/dev/null 2>&1; then
      echo "patched $file_path"
    fi
  done
