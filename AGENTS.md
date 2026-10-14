# Agent instructions (keykeeper-server)

This is the Keykeeper fork of Vaultwarden (`dani-garcia/vaultwarden`), AGPL-3.0. The project-wide rules live in the meta repo: [PimpinPumpkin/keykeeper/AGENTS.md](https://github.com/PimpinPumpkin/keykeeper/blob/main/AGENTS.md). Locally it is `../keykeeper/AGENTS.md`. Read it first.

## Rules specific to this fork

1. **Branch `keykeeper` is based on Vaultwarden release tags.** The current base is `1.37.4`, which supports web vault `v2026.8.0`. Move it only together with `keykeeper-clients`.
2. **CI runs from the meta repo.** Upstream's workflows were removed. [.github/workflows/keykeeper-ci.yml](.github/workflows/keykeeper-ci.yml) calls Tier 1 in the meta repo with this commit. When an upstream merge brings workflows back, delete them again.
3. **The server must never hold a key that can decrypt a vault.** Any change that would need one goes into DESIGN.md first.
4. **Every endpoint change needs a permission-matrix test** (every role, plus another org's member and an unauthenticated caller).
