# Contributing

## Development

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Run the server and desktop applications locally with the commands in the root README.

## Architectural rules

- Keep business rules in `crates/domain` or `crates/application`.
- Keep platform, network, process, and persistence code in adapters.
- Add a port before adding a second implementation of an external concern.
- Do not let UI or transport DTOs become domain entities.
- Document API and authorization changes.
- Never commit credentials, personal session data, machine-specific paths, or generated databases.

## Pull requests

Explain the user-facing behavior, affected boundary, security implications, and test coverage. Include migration notes for persisted data or API changes.
