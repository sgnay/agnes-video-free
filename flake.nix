{
  description = "agnes-video-free：把故事文本变成带旁白/字幕的短视频（Agnes Video V2.0 + edge-tts + ffmpeg）";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        # 运行时依赖：ffmpeg（含 ffprobe）用于旁白时长探测与成片组装（M2）
        runtimeDeps = with pkgs; [ ffmpeg ];
        agnesVideoFree = pkgs.rustPlatform.buildRustPackage {
          pname = "agnes-video-free";
          version = "0.1.0";
          src = ./.;
          cargoLock = {
            lockFile = ./Cargo.lock;
          };
          nativeBuildInputs = with pkgs; [
            pkg-config
            makeWrapper
          ];
          # 固定 CA 证书，保证 edge-tts / Agnes API（reqwest）TLS 可用
          SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
          postInstall = ''
            # 随包安装 OFL 字体（思源黑体），供字幕渲染（M2）与本地资源引用
            mkdir -p $out/share/agnes-video-free/fonts
            cp -r assets/fonts/. $out/share/agnes-video-free/fonts/
            # 运行时 PATH 注入 ffmpeg/ffprobe，并固定 CA 证书与字体目录
            wrapProgram $out/bin/agnes-video-free \
              --prefix PATH : ${pkgs.lib.makeBinPath runtimeDeps} \
              --set SSL_CERT_FILE ${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt \
              --set AGNES_VIDEO_FREE_FONTS $out/share/agnes-video-free/fonts
          '';
          meta = with pkgs.lib; {
            description = "用 Agnes Video V2.0 + edge-tts + ffmpeg 把故事文本变成竖屏短视频（TikTok / 小红书 / 微博）";
            mainProgram = "agnes-video-free";
            platforms = platforms.linux;
          };
        };
      in
      {
        packages.default = agnesVideoFree;

        apps.default = {
          type = "app";
          program = "${agnesVideoFree}/bin/agnes-video-free";
          meta.description = "agnes-video-free：故事文本 → 短视频";
        };

        devShells.default = pkgs.mkShell {
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
        };
      });
}
