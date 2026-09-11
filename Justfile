# Agentifi build recipes.
# The explicit Clang variables avoid SteamOS selecting the Homebrew GCC shim,
# which does not have access to the system C headers required by ring.

set shell := ["bash", "-cu"]

linux_cc := "CC=clang CXX=clang++ CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=clang"

# Build the cross-platform desktop application.
desktop-build:
    env {{linux_cc}} cargo build --package agentifi-desktop

# Build the server/CLI application.
cli-build:
    env {{linux_cc}} cargo build --package agentifi-server

# Build and run the desktop application.
desktop: desktop-build
    env {{linux_cc}} cargo run --package agentifi-desktop
