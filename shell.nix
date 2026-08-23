{pkgs ? import <nixpkgs> {}}:

let
  # Graphics and display dependencies for GPUI
  gpuiDeps = with pkgs; [
    wayland
    wayland-protocols
    libx11
    libxcursor
    libxrandr
    libxrender
    libxcb
    libxkbcommon
    fontconfig
    freetype
    vulkan-loader
    mesa
    libglvnd
    libinput
    tesseract
    pkg-config
  ];
in
pkgs.mkShell {
  name = "simple-ocr-dev-shell";

  buildInputs = [ pkgs.rustc pkgs.cargo ] ++ gpuiDeps;

  shellHook = ''
    echo "OCR-GPUI development environment ready"
    echo "Tools: rustc $(rustc --version), cargo $(cargo --version)"
    echo "Tesseract: $(tesseract --version | head -1)"
    echo "Wayland: $WAYLAND_DISPLAY"
  '';

  # Runtime environment for the built binary
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath gpuiDeps;
  VK_ICD_FILENAMES = "${pkgs.mesa}/share/vulkan/icd.d/radeon_icd.x86_64.json";
}