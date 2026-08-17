# Nix development shell for building xav.
#
# Provides the native toolchain that build.sh assembles for pacman/dnf:
#   - Rust nightly (via rustup, same as ./build.sh)
#   - clang/llvm/lld/compiler-rt (shared toolchain, matching build.sh CC=clang)
#   - nasm (asm/* built by build.rs via nasm-rs)
#   - cmake / ninja / meson / pkg-config (+ pkgconf, which build.sh probes)
#   - ffmpeg (used by build.sh's PGO training-clip prep)
#   - autotools, gcc/g++ + static libc/libstdc++ for the static external deps
#
# Usage:
#     nix-shell
#     ./build.sh
#
# NOTE: this is a classic nix expression (no flake). Linux x86_64/aarch64 only,
# matching the Linux-targeted build.sh.
#
# IMPORTANT: nixpkgs is PINNED to a single revision (below) rather than using
# <nixpkgs>/NIX_PATH, because a mixed channel/NIX_PATH resolves to mutually
# incompatible store paths (e.g. clang, gcc's libgcc_s, and glibc's ld.so from
# different revisions). That mismatch breaks ALL dynamically-linked binaries on
# glibc >= 2.39 with:
#   "IFUNC symbol 'memset' referenced in 'libgcc_s.so.1' is defined in the
#    executable and creates an unsatisfiable circular dependency."
# Pinning keeps every toolchain component on one coherent snapshot.
#
# To bump nixpkgs, change the rev (full commit) below.

{ pkgs ? import (builtins.fetchGit {
    url = "https://github.com/NixOS/nixpkgs";
    rev = "e5bdc4a41d4c072fe1e3787eaa0320a384741d44";
  }) { }
}:

let
  # use the stable LLVM toolchain; the bleeding-edge llvmPackages_latest can
  # break dynamic linking on newer glibc.
  llvm = pkgs.llvmPackages;
in
pkgs.mkShell {
  packages = with pkgs; [
    # Rust toolchain
    rustup

    # C/C++ compile + link
    llvm.clang
    llvm.lld
    llvm.llvm
    llvm.compiler-rt
    llvm.libunwind
    gcc
    binutils

    # static C++/C runtime for the mostly-static link
    stdenv.cc.cc.lib
    glibc.static

    # asm + C build systems
    nasm
    cmake
    ninja
    meson
    pkg-config
    pkgconf
    gnumake
    autoconf
    automake
    libtool

    # misc
    curl
    ffmpeg
    git
    python3
  ];

  # build.sh (setup_toolchain) independently exports its own CC/CXX/LD, but
  # expose the same names in case build.rs / nasm-rs query them.
  CC = "${llvm.clang}/bin/clang";
  CXX = "${llvm.clang}/bin/clang++";
  LD = "${llvm.lld}/bin/ld.lld";
  AR = "${llvm.llvm}/bin/llvm-ar";
  NM = "${llvm.llvm}/bin/llvm-nm";
  RANLIB = "${llvm.llvm}/bin/llvm-ranlib";
  STRIP = "${llvm.llvm}/bin/llvm-strip";
  OBJCOPY = "${llvm.llvm}/bin/llvm-objcopy";

  shellHook = ''
    export CARGO_HOME="''${CARGO_HOME:-''$HOME/.cargo}"
    export RUSTUP_HOME="''${RUSTUP_HOME:-''$HOME/.rustup}"
    export PATH="''$CARGO_HOME/bin:''$PATH"

    if ! command -v cargo >/dev/null 2>&1 || ! rustup show active-toolchain >/dev/null 2>&1; then
      echo "Installing Rust nightly toolchain via rustup..."
      rustup toolchain install nightly --profile minimal
      rustup default nightly
    fi
    rustup component add rust-src --toolchain nightly >/dev/null 2>&1 || true

    echo "xav nix shell ready."
    echo "  rustc: $(rustc --version 2>/dev/null)"
    echo "  clang: $(clang --version 2>/dev/null | head -1)"
  '';
}
