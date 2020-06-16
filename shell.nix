{
  pkgs ? import (fetchTarball https://github.com/NixOS/nixpkgs-channels/archive/nixos-unstable.tar.gz) {},
  ...
}:

with pkgs;

mkShell {
  buildInputs = [
    cargo
    clippy
    rustc
    rustfmt
  ];
}
