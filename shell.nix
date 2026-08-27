# 传统 nix-shell 入口（无 Flakes 时的回退），与 flake.nix 的 devShells.default 等价。
{ pkgs ? import <nixpkgs> { } }:
let
  # ffmpeg（含 ffprobe）用于视觉片段拼接、混音和字幕组装
  runtimeDeps = with pkgs; [ ffmpeg ];
in
pkgs.mkShell {
  name = "agnes-video-free-dev-shell";
  packages = with pkgs; [
    rustc
    cargo
    rustfmt
    clippy
    pkg-config
  ] ++ runtimeDeps;
  SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
  shellHook = ''
    echo "agnes-video-free NixOS 开发环境已就绪"
    echo "工具: $(rustc --version), $(cargo --version), $(ffmpeg -version | head -n1)"
  '';
}
