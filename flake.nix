{
  description = "codesearch — fast, local semantic code search with BM25 + vector embeddings + tree-sitter AST";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    flake-utils.url = "github:numtide/flake-utils";
    substrate = {
      url = "github:pleme-io/substrate";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    devenv = {
      url = "github:cachix/devenv";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, substrate, ... }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ substrate.rustOverlays.${system}.rust ];
      };
      lib = pkgs.lib;

      darwinBuildInputs = (import "${substrate}/lib/darwin.nix").mkDarwinBuildInputs pkgs;

      rustPlatform = pkgs.makeRustPlatform {
        rustc = pkgs.fenixRustToolchain;
        cargo = pkgs.fenixRustToolchain;
      };

      onnxruntime = pkgs.onnxruntime;

      codesearch = rustPlatform.buildRustPackage {
        pname = "codesearch";
        version = "0.1.142";
        src = ./.;

        # cargoHash: fixed-output vendor dir (no broken symlinks under follows)
        cargoHash = "sha256-zVnw9PkS3GSAJcO4g9NzlshQbdTAaHJbE+sOUrakjd8=";

        nativeBuildInputs = with pkgs; [
          protobuf
          pkg-config
          cmake
        ];

        buildInputs = [ onnxruntime ] ++ darwinBuildInputs;

        ORT_LIB_LOCATION = "${onnxruntime}/lib";
        ORT_PREFER_DYNAMIC_LINK = "1";

        preBuild = ''
          export CARGO_TARGET_DIR="$(pwd)/target"
        '';

        postFixup = lib.optionalString pkgs.stdenv.isDarwin ''
          install_name_tool -add_rpath "${onnxruntime}/lib" "$out/bin/codesearch"
        '';

        doCheck = false;
      };
    in {
      packages = {
        default = codesearch;
        inherit codesearch;
      };

      devShells.default = pkgs.mkShell {
        nativeBuildInputs = [
          pkgs.fenixRustToolchain
          pkgs.pkg-config
          pkgs.cmake
          pkgs.protobuf
        ];

        buildInputs = [ onnxruntime ] ++ darwinBuildInputs;

        ORT_LIB_LOCATION = "${onnxruntime}/lib";
        ORT_PREFER_DYNAMIC_LINK = "1";
        RUST_SRC_PATH = "${pkgs.fenixRustToolchain}/lib/rustlib/src/rust/library";
      };
    }) // {
      homeManagerModules.default = import ./module {
        hmHelpers = import "${substrate}/lib/hm-service-helpers.nix" { lib = nixpkgs.lib; };
      };
      overlays.default = final: prev: {
        codesearch = self.packages.${final.system}.default;
      };
    };
}
