asahi_dev_port := "49306"
asahi_dev_db_path := justfile_directory() / "asahi-dev.db"
asahi_dev_database_url := "sqlite://" + asahi_dev_db_path + "?mode=rwc"

install:
    cargo install --path ./crates/luna --force --locked

# Start the Asahi frontend dev server.
asahi-frontend:
    ASAHI_PORT='{{ asahi_dev_port }}' bun run --cwd '{{ justfile_directory() / "apps/asahi-web" }}' dev --host 127.0.0.1

_ensure-cargo-watch:
    @if ! cargo watch --version >/dev/null 2>&1; then cargo install cargo-watch; fi

# Start the Asahi backend against the repo-local development database.
asahi-backend: _ensure-cargo-watch
    ASAHI_PORT='{{ asahi_dev_port }}' ASAHI_DATABASE_URL='{{ asahi_dev_database_url }}' DATABASE_URL='{{ asahi_dev_database_url }}' ROCKET_ADDRESS=127.0.0.1 ROCKET_PORT='{{ asahi_dev_port }}' ASAHI_SKIP_WEB_BUILD=1 cargo watch -w crates/asahi -w crates/asahi-migration -w Cargo.toml -w Cargo.lock -x 'run -p asahi'
