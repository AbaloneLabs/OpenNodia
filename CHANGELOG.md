# Changelog

## Unreleased

### Added

- Native DEX route-candidate API that compares native orderbook, native AMM,
  Tinyman, and Pact quote candidates without executing transactions.
- External Tinyman/Pact exact-input swap prepare/submit path guarded by local
  transaction-group validation and a disabled-by-default write gate.
- External LP position display for verified Tinyman/Pact pool LP assets.
- Folks lending-backed Pact pool display metadata with explicit risk notes.
- Runtime secret initialization scripts for Linux/macOS shells and Windows
  PowerShell.
- Public release documentation for installation, backup/restore, migration,
  troubleshooting, service management, reverse proxy, supply chain, and Testnet
  validation.
- CI workflow for Rust/frontend checks, secret scan, npm audit, container
  build, cross-platform smoke checks, and supply-chain artifact generation.

### Changed

- Frontend development dependencies upgraded to supported Svelte/Vite versions.
- External AMM swaps remain opt-in via `external_liquidity.swap_enabled`; external LP
  add/remove remains separately opt-in via `external_liquidity.liquidity_enabled`.
- Folks-backed pools are shown as quote-only unless adapter swap verification
  is explicitly implemented.

### Security

- `scripts/check-secrets.sh` blocks current installation secrets and common
  plaintext credential patterns from being committed.
- Release QA now includes npm audit and optional Rust/container/SBOM checks.

### Known limitations before v1

- Mainnet write paths require independent audit sign-off.
- macOS and Windows full install/upgrade/reboot validation are planned but not
  yet proven on this Linux development host.
- External LP add/remove execution is not yet enabled from OpenNodia.
- Folks utilization and incentive APR are displayed only when independently
  verified; unverified reward APR is not folded into yield.
