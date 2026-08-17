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

{ pkgs ? import <nixpkgs> { } }:

let
  llvm = pkgs.llvmPackages_latest;
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
