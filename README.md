# PassBundle server

The server for [PassBundle](https://github.com/PimpinPumpkin/Passbundle), a password manager for individuals, families, and managed service providers (MSPs).

It's derived from [Vaultwarden](https://github.com/dani-garcia/vaultwarden), a server for the Bitwarden client API written in Rust, and adds what PassBundle needs: MSPs with their staff and client organizations, custom roles, default policies, item templates every client gets, onboarding through an RMM install command, and an audit log for all of it. Vaults are end-to-end encrypted: the server never holds a key that can decrypt one.

> PassBundle is in development and not ready for production use.

## Building

```sh
cargo build --features sqlite        # SQLite, for development and tests
cargo build --features postgresql    # PostgreSQL, for production
```

The tests for this server live in the [PassBundle repository](https://github.com/PimpinPumpkin/Passbundle): API tests, permission matrices, and end-to-end tests that run against this server and the [PassBundle apps](https://github.com/PimpinPumpkin/passbundle-clients). CI here runs that suite against each commit.

## Security

See [SECURITY.md](SECURITY.md).

## License and credits

AGPL-3.0, see [LICENSE.txt](LICENSE.txt). Based on Vaultwarden by Daniel García and its contributors, whose work this builds on.

PassBundle isn't affiliated with or endorsed by Bitwarden, Inc. or the Vaultwarden project. Bitwarden is a trademark of Bitwarden, Inc.
