with import ./nix/pkgs.nix {};
let 
  merged-openssl = symlinkJoin { name = "merged-openssl"; paths = [ openssl.out openssl.dev ]; };
in stdenv.mkDerivation rec {
  name = "matcher";
  env = buildEnv { name = name; paths = buildInputs; };

  buildInputs = [
    rustup
    openssl
    cmake
  ];
  shellHook = ''
  export OPENSSL_DIR="${merged-openssl}"
  '';
  LD_LIBRARY_PATH = lib.makeLibraryPath [ openssl ];
}

