# OpenNodia Security

## Installation secrets

Run the initialization helper once before starting Docker Compose:

```bash
./scripts/init-secrets.sh
docker compose up -d
```

On native Windows PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\init-secrets.ps1
docker compose up -d
```

The helper creates a unique algod API token and Indexer PostgreSQL password
under:

```text
${XDG_CONFIG_HOME:-$HOME/.config}/opennodia/secrets
```

The directory is mode `0700` and each secret file is mode `0600`. The generated
`.env` file stores only non-secret settings and the absolute path to this
directory. Set `OPENNODIA_SECRETS_DIR` before running the platform helper to
use another absolute path. The PowerShell helper writes a forward-slash Windows
path to `.env` for Docker Compose interpolation.

Both initialization helpers are idempotent: a non-empty existing secret file
is reused and never overwritten. Container restarts, Docker restarts, Compose
re-creation, and host reboots therefore keep the same credentials. Deleting or
losing the secret directory can prevent an existing PostgreSQL volume from
starting correctly, so include it in the installation backup.

Docker Compose mounts the values as read-only secret files. They are not
included in normal `docker compose config` output, container configuration
environment variables, Indexer process arguments, or the persistent Conduit
configuration.

Back up the secret directory separately from the public source repository.
Do not publish it or place it inside a shared workspace.

## Git protection

`init-secrets.sh` enables the repository's pre-commit hook. The hook runs:

```bash
./scripts/check-secrets.sh
```

It blocks commits containing the current installation secrets and several
common plaintext credential patterns. This is a defense-in-depth check, not a
replacement for reviewing staged changes.

## Working with coding agents

Agents working on this repository should follow these rules:

- Do not read or print `.env`, `/run/secrets`, the configured secret directory,
  algod token files, kmd token files, wallet mnemonics, PIN data, or database
  passwords.
- Do not enable shell tracing (`set -x`) around commands that consume secrets.
- Do not pass credentials in command-line arguments.
- Prefer `docker compose config --quiet` when only validation is required.
- Redact environment and process inspection output before returning it.
- Run `./scripts/check-secrets.sh` before every commit.

An agent with unrestricted host filesystem or Docker access can deliberately
read runtime secrets. Preventing that requires an OS-level permission boundary:
run the agent as a separate unprivileged user without access to the secret
directory, Docker socket, algod data, kmd data, or production database. The
project's file-based design prevents accidental disclosure during ordinary
development workflows; it cannot override permissions already granted to an
agent.

## Reporting vulnerabilities

Do not open a public issue containing credentials or exploit details. Contact
the maintainers privately and rotate any potentially exposed credential.
