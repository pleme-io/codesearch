{
  description = "codesearch — fast, local semantic code search with BM25 + vector embeddings + tree-sitter AST";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.05";
    flake-utils.url = "github:numtide/flake-utils";
    substrate = {
      url = "github:pleme-io/substrate";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, substrate, ... }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ substrate.overlays.${system}.rust ];
      };
      lib = pkgs.lib;

      rustPlatform = pkgs.makeRustPlatform {
        rustc = pkgs.fenixRustToolchain;
        cargo = pkgs.fenixRustToolchain;
      };

      # ort-sys 2.0.0-rc.11 requires ORT API version 23 (onnxruntime 1.23+).
      # nixpkgs 25.05 has 1.23.2.
      onnxruntime = pkgs.onnxruntime;

      codesearch = rustPlatform.buildRustPackage {
        pname = "codesearch";
        version = "0.1.142";
        src = ./.;

        cargoLock.lockFile = ./Cargo.lock;

        nativeBuildInputs = with pkgs; [
          protobuf
          pkg-config
          cmake
        ];

        buildInputs = [
          onnxruntime
        ] ++ lib.optionals pkgs.stdenv.isDarwin (
          [ pkgs.libiconv ]
          ++ (if pkgs ? apple-sdk
              then [ pkgs.apple-sdk ]
              else lib.optionals (pkgs ? darwin) (
                with pkgs.darwin.apple_sdk.frameworks; [
                  Security
                  SystemConfiguration
                ]
              ))
        );

        # Tell the `ort` crate to use pre-built ONNX Runtime from nixpkgs
        # instead of downloading binaries at build time (blocked by Nix sandbox).
        # All TLS uses rustls — no OpenSSL/native-tls dependency.
        ORT_LIB_LOCATION = "${onnxruntime}/lib";
        ORT_PREFER_DYNAMIC_LINK = "1";

        # The Nix sandbox unpacks source into $NIX_BUILD_TOP/source/ but cargo
        # writes to $NIX_BUILD_TOP/target/ — the install hook expects target/ to
        # be relative to pwd (source/). Fix by anchoring CARGO_TARGET_DIR.
        preBuild = ''
          export CARGO_TARGET_DIR="$(pwd)/target"
        '';

        # Ensure the ONNX Runtime dylib is found at runtime via rpath
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

        buildInputs = [
          onnxruntime
        ] ++ lib.optionals pkgs.stdenv.isDarwin (
          [ pkgs.libiconv ]
          ++ (if pkgs ? apple-sdk
              then [ pkgs.apple-sdk ]
              else lib.optionals (pkgs ? darwin) (
                with pkgs.darwin.apple_sdk.frameworks; [
                  Security
                  SystemConfiguration
                ]
              ))
        );

        ORT_LIB_LOCATION = "${onnxruntime}/lib";
        ORT_PREFER_DYNAMIC_LINK = "1";
        RUST_SRC_PATH = "${pkgs.fenixRustToolchain}/lib/rustlib/src/rust/library";
      };
    }) // {
      # Non-per-system outputs
      homeManagerModules.default = import ./module {
        hmHelpers = import "${substrate}/lib/hm-service-helpers.nix" { lib = nixpkgs.lib; };
      };
      overlays.default = final: prev: {
        codesearch = self.packages.${final.system}.default;
      };
    };
}
