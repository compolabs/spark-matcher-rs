{ pkgs ? import <nixpkgs> {} }:

let
  lib = pkgs.lib;
  isDarwin = pkgs.stdenv.isDarwin;
  merged-openssl = pkgs.symlinkJoin {
    name = "merged-openssl";
    paths = [ pkgs.openssl.out pkgs.openssl.dev ];
  };
in pkgs.stdenv.mkDerivation rec {
  name = "matcher";
  env = pkgs.buildEnv { name = name; paths = buildInputs; };

  buildInputs = [
    pkgs.rustup
    pkgs.openssl
    pkgs.cmake
    pkgs.postgresql
    pkgs.sqlx-cli
    pkgs.libiconv
  ] ++ lib.optional isDarwin pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
    ++ lib.optional isDarwin pkgs.darwin.apple_sdk.frameworks.CoreFoundation
    ++ lib.optional isDarwin pkgs.darwin.apple_sdk.frameworks.Security;

  shellHook = ''
    export OPENSSL_DIR="${merged-openssl}"

    echo "Setting up the environment for Matcher..."

    export RUST_LOG=info
    echo "RUST_LOG is set to $RUST_LOG"

    export PGDATA=./pgsql-data
    export PGHOST=/tmp
    export PGPORT=5432

    if [ ! -d "$PGDATA" ]; then
      mkdir -p $PGDATA
      pg_ctl init -D $PGDATA
      echo "unix_socket_directories = '$PGHOST'" >> $PGDATA/postgresql.conf
      echo "logging_collector = off" >> $PGDATA/postgresql.conf
      echo "log_min_messages = fatal" >> $PGDATA/postgresql.conf
      pg_ctl start -D $PGDATA -o "-k $PGHOST" > /dev/null 2>&1
      psql -d postgres -c "CREATE ROLE metagm LOGIN CREATEDB PASSWORD 'metagm';"
      psql -d postgres -c "CREATE DATABASE matcher_db OWNER metagm ENCODING 'UTF8';"
    else
      pg_ctl start -D $PGDATA -o "-k $PGHOST" > /dev/null 2>&1
    fi

    export DATABASE_URL="postgresql://metagm:metagm@localhost:$PGPORT/matcher_db"

    if [ -d ./migrations ]; then
      sqlx database create
      sqlx migrate run
      echo "Database migrations have been applied."
    fi

    function cleanup {
      echo "Cleaning up..."
      pg_ctl stop -D $PGDATA > /dev/null 2>&1
    }
    trap cleanup EXIT

    echo "Matcher environment is ready. Database URL: $DATABASE_URL"
  '';

  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [ pkgs.openssl pkgs.libiconv ];
}
