{ pkgs ? import <nixpkgs> {} }:

let
  rustOverlay = builtins.fetchTarball "https://github.com/oxalica/rust-overlay/archive/master.tar.gz";
  pkgs = import <nixpkgs> {
    overlays = [
      (import rustOverlay)
    ];
  };
  rustVersion = "1.85.0";

  # Create a Python environment with required packages
  pythonEnv = pkgs.python3.withPackages (ps: with ps; [
    matplotlib
    numpy
    pandas
    seaborn
  ]);

  # List of extra tools
  toolList = with pkgs; [
    rust-analyzer
    cargo-watch
    cargo-outdated

    wrk
    bc
    lsof
  ];

  # Function to get the name of a derivation
  getName = drv: drv.pname or drv.name or "unknown";

  # Generate the tool list string
  toolListString = builtins.concatStringsSep "\n  - " (map getName toolList);

in
pkgs.mkShell {
  buildInputs = with pkgs; [
    (rust-bin.stable.${rustVersion}.default.override {
      extensions = [ "rust-src" ];
    })
    pythonEnv
  ] ++ toolList;

shellHook = ''
    # Welcome message
    printf "\n\033[1;34m=============================================\033[0m"
    printf "\n\033[1;32m🦀 Rust Development Environment Activated 🦀\033[0m"
    printf "\n\033[1;34m=============================================\033[0m"
    printf "\n\033[1;33m• Rust Version: ${rustVersion}\033[0m"
    printf "\n\033[1;33m• Available Tools:\033[0m"
    printf "\n  - ${toolListString}"
    printf "\n\033[1;34m=============================================\033[0m"

    printf "\n\033[1;33m• Checking for any outdated packages...\033[0m\n"
    cargo outdated --root-deps-only
  '';
}
