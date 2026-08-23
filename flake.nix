{
  description = "Simple Linux desktop OCR app using GPUI (Rust)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        
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
          poppler-utils
          dbus
        ];

        # Build the Rust package
        ocrGpui = pkgs.rustPlatform.buildRustPackage {
          pname = "simple-ocr";
          version = "0.1.0";

          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = with pkgs; [
            pkg-config
            makeWrapper
            poppler-utils
            tesseract
          ];

          buildInputs = gpuiDeps;

          # Wrap binary with runtime library paths and Vulkan ICD
          postInstall = ''
            wrapProgram $out/bin/simple-ocr \
              --prefix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath gpuiDeps} \
              --prefix VK_ICD_FILENAMES : ${pkgs.mesa}/share/vulkan/icd.d/radeon_icd.x86_64.json
          '';
        };
      in {
        devShells.default = pkgs.mkShell {
          name = "simple-ocr-dev-shell";
          
          buildInputs = [ pkgs.pkg-config ] ++ gpuiDeps;
          packages = [ pkgs.rustc pkgs.cargo ];

          shellHook = ''
            echo "OCR-GPUI development environment ready"
            echo "Tools: rustc $(rustc --version), cargo $(cargo --version)"
            echo "Tesseract: $(tesseract --version | head -1)"
            echo "Wayland: $WAYLAND_DISPLAY"
          '';
        };

        packages.default = ocrGpui;

        apps.default = {
          type = "app";
          program = "${ocrGpui}/bin/simple-ocr";
        };
      });
}