{ pkgs, lib, ... }:
{
  packages = with pkgs; [
    rustc
    cargo
    rustfmt
    clippy
    pkg-config
    mpv
    ffmpeg
    lua5_4
  ] ++ lib.optionals pkgs.stdenv.isLinux [ chromium libxkbcommon wayland vulkan-loader ];

  scripts = {
    build.exec = "cargo build --release --locked";
    check.exec = "cargo check --workspace --locked";
    lint.exec = "cargo clippy --workspace --all-targets --locked -- -D warnings";
    fmt.exec = "cargo fmt --all";
    test.exec = "cargo test --workspace --locked";
    run-app.exec = "cargo run -p vkpl --";
    run-gui.exec = ''
      export LD_LIBRARY_PATH="${lib.makeLibraryPath [ pkgs.wayland pkgs.libxkbcommon pkgs.vulkan-loader ]}''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
      exec cargo run -p vkpl-gui --
    '';
    package-local.exec = ''
      target="$(rustc -vV | sed -n 's/^host: //p')"
      cargo build --release --locked -p vkpl --target "$target"
      bash scripts/package-release.sh "''${VERSION:-dev}" "$target"
    '';
    build-nix.exec = "nix build .#vkpl";
    run-nix.exec = "nix run . --";
    flake-check.exec = "nix flake check";
  };
}
