{
  description = "Lom, a native Sophia desktop shell: pinned Nix build and gate (niltempus n002)";

  inputs = {
    # The reviewed nixpkgs revision shared by the niltempus product flakes.
    nixpkgs.url = "github:NixOS/nixpkgs/c59305bab2065cfecc4944690d9eedbb56f3a9fa";
    crane.url = "github:ipetkov/crane";
    # Supplies the exact toolchain rust-toolchain.toml names; nixpkgs has a newer one.
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, crane, rust-overlay }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ rust-overlay.overlays.default ];
      };
      toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;

      # The whole tree, as the gate sees it: tests read snapshots and assets
      # beside the sources. The flake files are left out, so editing them does
      # not rebuild Lom.
      src = pkgs.lib.cleanSourceWith {
        src = pkgs.lib.cleanSource ./.;
        filter = path: _: !(builtins.elem (baseNameOf path) [ "flake.nix" "flake.lock" ]);
      };
      common = {
        inherit src;
        strictDeps = true;
        nativeBuildInputs = [ pkgs.pkg-config ];
        buildInputs = [ pkgs.fontconfig ];
      };
      cargoArtifacts = craneLib.buildDepsOnly common;

      # The binary niltempus builds: cargo build --locked --release.
      lom = craneLib.buildPackage (common // {
        inherit cargoArtifacts;
        doCheck = false;
        # wgpu loads the Vulkan loader with dlopen; give it the pinned one.
        postFixup = ''
          patchelf --add-rpath ${pkgs.vulkan-loader}/lib $out/bin/lom
        '';
      });

      # tools/check.sh, offline: the same commands with warnings denied. The
      # layout audit (also run by tests/source_layout.rs) reads git's file
      # list, and its own tests make repositories. The flake source is the
      # tracked tree, so it is indexed again.
      gateArgs = common // {
        inherit cargoArtifacts;
        RUSTFLAGS = "-D warnings";
        RUSTDOCFLAGS = "-D warnings";
        nativeBuildInputs = common.nativeBuildInputs ++ [ pkgs.python3 pkgs.git ];
        preBuild = ''
          export HOME=$TMPDIR
          git config --global user.name nix-check
          git config --global user.email nix-check@localhost
          git init -q && git add -A
        '';
      };
    in
    {
      packages.${system} = {
        inherit lom;
        default = lom;
      };

      checks.${system} = {
        inherit lom;
        format = craneLib.cargoFmt { inherit src; };
        clippy = craneLib.cargoClippy (gateArgs // {
          cargoClippyExtraArgs = "--workspace --all-targets -- -D warnings";
        });
        # The four signal tests start the test binary under bwrap with the host
        # layout (/usr, /etc); a Nix-built binary also needs /nix/store there,
        # which Nix's build sandbox cannot give them. They run in the host gate.
        tests = craneLib.cargoTest (gateArgs // {
          cargoTestExtraArgs = "--workspace --all-targets -- --skip cli::serve::tests::sig";
        });
        doctests = craneLib.cargoTest (gateArgs // {
          pnameSuffix = "-doc";
          cargoTestExtraArgs = "--workspace --doc";
        });
        tools = craneLib.mkCargoDerivation (gateArgs // {
          pnameSuffix = "-tools";
          doInstallCargoArtifacts = false;
          buildPhaseCargoCommand = ''
            python3 -B -m unittest discover -s tools/tests -p 'test_*.py'
            python3 -B tools/audit_source_layout.py
            python3 -B tools/audit_dependencies.py
          '';
          installPhaseCommand = "mkdir -p $out";
        });
      };

      # Tools and environment only: no filesystem, device or network isolation.
      devShells.${system}.default = craneLib.devShell {
        packages = [ pkgs.pkg-config pkgs.fontconfig pkgs.python3 ];
        LD_LIBRARY_PATH = "${pkgs.vulkan-loader}/lib";
      };
    };
}
