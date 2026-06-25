<div align="center">

> **⚠️ WARNING: This project is under active development.**
> Do not build or run in production. Features are incomplete and may contain security vulnerabilities.
>
> The default network is **Algorand Testnet**. You can get free test ALGO from the
> [Algorand Testnet Dispenser](https://bank.testnet.algorand.network/) and test USDC
> from the [Circle Testnet Faucet](https://faucet.circle.com/).

<br>

<img src="assets/opennodia-logo.svg" alt="OpenNodia" width="180" height="180"/>

# OpenNodia

**Your node. Your assets. Your market.**

An open-source, self-hosted DEX node for Algorand.

[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Language: Rust](https://img.shields.io/badge/Language-Rust-dea584.svg)](https://www.rust-lang.org/)
[![Status: Pre-Alpha](https://img.shields.io/badge/Status-Pre--Alpha-red.svg)](#roadmap)
[![Platform: Algorand](https://img.shields.io/badge/Platform-Algorand-00d4aa.svg)](https://algorand.org/)

<br>

<a href="README.md"><b>English</b></a> ·
<a href="README.ko.md">한국어</a> ·
<a href="README.zh.md">中文</a> ·
<a href="README.ja.md">日本語</a>

<br><br>

</div>

---

## Overview

OpenNodia lets you **run your own Algorand node**, verify ASA assets against your own copy of the ledger, and operate a **non-custodial DEX** — all from a self-hosted server you control.

No centralized exchange. No custody. No third-party API deciding what's "true". Just your node, your assets, your rules.

```
┌──────────────────────────────────────────────────────────────────┐
│                        Your PC / NAS / VPS                       │
│                                                                  │
│   ┌──────────────┐         ┌──────────────────────────────────┐  │
│   │              │  read   │       OpenNodia Daemon           │  │
│   │   Algorand   │◄────────┤   ┌──────────┬───────────────┐   │  │
│   │    Node      │         │   │ Web UI   │  Asset Mgr    │   │  │
│   │   (algod)    │────────►┤   ├──────────┼───────────────┤   │  │
│   │              │  events │   │ PIN Auth │  DEX Engine   │   │  │
│   └──────────────┘         │   ├──────────┼───────────────┤   │  │
│                            │   │ Events   │  SQLite Cache │   │  │
│                            │   └──────────┴───────────────┘   │  │
│                            └───────────────┬──────────────────┘  │
└────────────────────────────────────────────┼─────────────────────┘
                                             │ HTTP / WebSocket
                          ┌──────────────────┼──────────────────┐
                          ▼                  ▼                  ▼
                    ┌──────────┐       ┌──────────┐       ┌──────────┐
                    │ Desktop  │       │  Mobile  │       │  Tablet  │
                    │ Browser  │       │ Browser  │       │ Browser  │
                    └──────────┘       └──────────┘       └──────────┘
```

## Why OpenNodia?

Most Algorand users rely on hosted explorers and third-party DEXes. OpenNodia flips that model:

| | Traditional | OpenNodia |
|---|---|---|
| **Ledger source** | Public API (rate-limited, third-party) | Your own node (local, unlimited) |
| **Asset custody** | Exchange holds your keys | Non-custodial — you always hold your keys |
| **DEX** | Centralized matching server | Self-hosted atomic swaps, no middleman |
| **Trust** | "Trust the operator" | "Trust your own node" |
| **Privacy** | Account data sent to external APIs | Queries stay local by default |

## Key Features

- **Local-first ledger** — Read from your own `algod` node. Public APIs are only a fallback, never the primary source.
- **Non-custodial by design** — OpenNodia never holds your assets. Every transaction requires your explicit signature.
- **Self-hosted DEX** — Trade ASAs via atomic swaps. No central matching engine, no order book server you don't control.
- **Wallet management** — Create or import Algorand wallets (kmd-backed). Multiple wallets, address generation, and PIN-protected access.
- **Send & receive** — Transfer ALGO and ASAs with a human-readable preview before signing. ASA opt-in support.
- **PIN-gated web access** — A lightweight web dashboard, protected by a PIN (argon2id-hashed). Change it anytime.
- **AI assistant** *(planned)* — An optional chatbot that helps you use OpenNodia. Connect your own LLM and ask it to verify ASA IDs, explain assets, or answer questions. It can read and explain, but can never sign transactions or place buy/sell orders.

## Core Principles

| Principle | What it means |
|---|---|
| **Local-first** | Your node verifies the ledger, not a public API. |
| **Non-custodial** | Your assets are never held by OpenNodia. |
| **Self-hosted** | You run the daemon on your own PC, NAS, or VPS. |
| **Open-source** | Apache-2.0 licensed, fully transparent. |
| **Human-approved** | Every transaction requires explicit user approval. |
| **Spot assets only** | Atomic swaps for freely transferable ASAs. No derivatives. |
| **AI assistant** | An optional chatbot connected to your own LLM. It can read and explain, but can never sign transactions or place trades. |

## Components

| Component | Description | Status |
|---|---|---|
| **OpenNodia Node** | Algorand node daemon and ledger connector | :white_check_mark: |
| **OpenNodia Assets** | ASA asset management terminal with policy grading | :white_check_mark: |
| **OpenNodia DEX** | Non-custodial, self-hosted spot DEX with atomic swaps | :white_check_mark: |
| **OpenNodia Mobile** | Mobile web / PWA companion | :hourglass: |

## Tech Stack

| Layer | Technology |
|---|---|
| **Backend** | [Rust](https://www.rust-lang.org/) (edition 2021, MSRV 1.80) |
| **Web framework** | [axum](https://github.com/tokio-rs/axum) + [tower-http](https://github.com/tower-rs/tower-http) |
| **Blockchain** | [Algorand](https://algorand.org/) (algod + kmd REST API) |
| **Smart Contracts** | TEAL v8 LogicSig (source compiled by algod) |
| **Database** | SQLite ([rusqlite](https://github.com/rusqlite/rusqlite)) — local orderbook & cache |
| **Frontend** | [Svelte](https://svelte.dev/) + [Vite](https://vitejs.dev/) + [Tailwind CSS](https://tailwindcss.com/) |
| **Auth** | argon2id (PIN) + HMAC session tokens |
| **Architecture** | Cargo workspace monorepo |
| **License** | Apache-2.0 |

## Repository Structure

```
opennodia/
├── crates/
│   ├── opennodia-core/      # Shared types: Address, AssetId, MicroAlgo, Round
│   ├── opennodia-node/      # algod/kmd REST clients, node status, account/asset queries
│   ├── opennodia-assets/    # ASA management, policy grading (open/bridged/regulated)
│   ├── opennodia-swap/      # Atomic swap: escrow, tx builder, matching engine
│   ├── opennodia-amm/       # Native AMM math, contracts, pool transaction builders
│   ├── opennodia-dex/       # Local orderbook: SQLite persistence, on-chain event tracking
│   └── opennodia-server/    # HTTP orchestration, web UI, PIN auth, wallet mgmt, ledger APIs
├── frontend/                # Svelte SPA (multi-language: EN/KO/ZH/JA)
├── docker/                  # algod container entrypoint wrapper
├── Cargo.toml               # Workspace root
├── docker-compose.yml       # Node, bounded Indexer, PostgreSQL, and helper services
├── Dockerfile               # Multi-stage build (frontend + backend)
└── LICENSE                  # Apache-2.0
```

## Roadmap

| Milestone | Title | Status |
|---|---|:---:|
| **M0** | Monorepo Scaffold | :white_check_mark: |
| **M1** | Node & Web Server Foundation | :white_check_mark: |
| **M2** | Asset Dashboard | :white_check_mark: |
| **M3** | Atomic Swap Core | :white_check_mark: |
| **M4** | Local Orderbook DEX | :white_check_mark: |
| **M5** | Community DEX | :white_check_mark: |
| **M8** | Local Indexer | :white_check_mark: |
| **M9** | AI Agent Bridge | :hourglass: |
| **M10** | Mobile Web & PWA | :hourglass: |
| **M11** | Public Release (v1.0) | :hourglass: |

## Hardware Requirements

OpenNodia runs a participation node, a lightweight follower node, a Conduit
pipeline, PostgreSQL, and a read-only indexer API. These are the recommended
specs for a self-hosted setup.

### Recommended

| Resource | Testnet | Mainnet |
|----------|---------|---------|
| **CPU** | 4 cores | 8 cores |
| **RAM** | 8 GB | 16 GB |
| **Disk** | 100 GB SSD | 200 GB usable SSD (256 GB device recommended) |
| **Network** | 10 Mbps | 50 Mbps+ |

### Notes

- **Two algod nodes.** The participation node earns block rewards and relays
  transactions; the follower is a non-archival Conduit data source and local
  read-only current-state source with a reduced 2,000-round account-delta
  recovery window.
- **Disk type matters.** An SSD (or NVMe) is strongly recommended. Spinning
  disks make algod catchup and indexer bootstrapping painfully slow.
- **Bounded local Indexer.** The default database starts near the follower tip
  and retains 20,000 recent rounds. Transactions, participation rows, and block
  headers older than that window are pruned together.
- **Permanent wallet history.** Transactions involving registered OpenNodia
  wallet addresses are copied into a separate PostgreSQL schema before pruning.
  Older non-wallet history uses the configured public Indexer fallback.
- **Follower recovery window.** The follower retains 2,000 account-delta
  rounds by default. Increase `ALGOD_FOLLOWER_LOOKBACK` if Conduit may be
  offline for longer periods; retained deltas increase follower disk usage.
- **Mainnet budget.** A typical deployment is expected to use approximately
  120–180 GB including participation algod, follower, bounded PostgreSQL, and
  container overhead. A wallet with exchange-scale transaction volume can
  exceed this estimate because its selected history is permanent.
- **Block rewards are unaffected.** The participation node is completely
  independent of the follower node and Conduit pipeline. A 30,000+ ALGO node
  with valid online participation keys keeps proposing blocks and earning
  rewards regardless of whether the follower or indexer are running.

Check the live PostgreSQL footprint with:

```bash
docker compose exec postgres psql -U algorand -d indexer \
  -c "SELECT pg_size_pretty(pg_database_size(current_database()));"
docker system df -v
```

## Quick Start

> **OpenNodia is in active development.** The core stack (node connector, asset
dashboard, wallet management, send/receive, and local DEX) is functional on
Algorand Testnet. Use Docker Compose for the fastest setup.

### Option A: Docker Compose (recommended)

```bash
# Clone the repository
git clone https://github.com/AbaloneLabs/OpenNodia.git
cd OpenNodia

# Copy the sample config
cp opennodia.sample.toml opennodia.toml

# Generate installation-specific secrets outside the repository
./scripts/init-secrets.sh
# Windows PowerShell:
# powershell -ExecutionPolicy Bypass -File .\scripts\init-secrets.ps1

# Start the full stack:
#   algod (participation) + algod-follower + conduit + postgres + indexer + opennodia
docker compose up -d

# Open the web UI at the address printed by the initializer
# Example: http://192.168.1.20:30080
```

The initializer detects the primary private IPv4 address and publishes the web
UI only on that host interface, so other devices on the same LAN can connect
without exposing every interface through `0.0.0.0`. Set
`OPENNODIA_BIND_ADDRESS=127.0.0.1` before running the initializer to keep the UI
host-only, or set a specific interface address to override detection. Host and
network firewall rules remain authoritative for routed networks.

`init-secrets.sh` creates unique credentials under
`${XDG_CONFIG_HOME:-$HOME/.config}/opennodia/secrets` with restrictive file
permissions. `.env` contains only non-secret settings and the absolute secret
directory path. Docker receives credentials through read-only secret files, so
normal Compose rendering, container inspection, and Git operations do not print
their values. See [SECURITY.md](SECURITY.md) for the security boundary and
agent-working rules. Re-running the initializer or restarting the host reuses
the existing files; it does not generate new credentials. An existing explicit
`OPENNODIA_BIND_ADDRESS` is also preserved.

The first time you open the UI, you'll set a PIN. Then you can create or import
an Algorand wallet and start exploring.

Detailed install and operations documents:

- [Installation](INSTALL.md)
- [Security](SECURITY.md)

The stack runs the core containers plus one-shot bootstrap and retention
helpers:

| Service | Role |
|---------|------|
| `algod` | Participation node — consensus, block rewards, kmd, transaction relay |
| `algod-follower` | Lightweight follower node — streams block data to Conduit |
| `conduit` | Data pipeline — reads blocks from the follower, writes to PostgreSQL |
| `postgres` | Database — stores the indexed blockchain data |
| `indexer` | Read-only REST API — serves asset search and transaction history |
| `indexer-bootstrap` | One-shot initialization near the follower tip |
| `indexer-pruner` | Keeps local Indexer tables within 20,000 recent rounds |
| `opennodia` | The OpenNodia server + web UI |

The participation node is completely independent of the follower/Conduit/indexer
pipeline, so it does **not** affect block rewards or node consensus. While the
local Indexer is bootstrapping, OpenNodia automatically falls back to a public
Indexer. Once caught up, recent history comes from the local Indexer. Registered
wallet history is permanently cached in PostgreSQL, while older non-wallet
history remains available through the public fallback.

### Option B: Build from source

```bash
# Backend (requires Rust 1.88+)
cargo build --workspace
cargo run --bin opennodia-server -- --config opennodia.toml

# Frontend (separate terminal)
cd frontend
npm install
npm run dev    # http://localhost:5173 (proxies /api to :30080)
```

### Running tests

```bash
# All Rust tests (offline — no algod required)
cargo test --workspace
```

## Not in Scope

- Custody of user assets
- Investment advice
- Automatic / algorithmic trading
- Leverage, margin, futures, or derivatives
- Securities exchange
- Group chat
- AI-driven trading, order placement, or transaction signing
- Buy/sell actions initiated by AI

## Contributing

This project is in active development. We are not accepting pull requests at this time, but we welcome:

- :beetle: **Bug reports** — Open an [issue](https://github.com/AbaloneLabs/OpenNodia/issues)
- :brain: **AI-assisted analysis** — We welcome issues that use AI tools to identify security vulnerabilities, code smells, or improvement opportunities. Please describe the findings clearly.
- :bulb: **Ideas & feedback** — Start a [discussion](https://github.com/AbaloneLabs/OpenNodia/discussions)
- :globe_with_meridians: **Translations** — Help improve the multilingual READMEs

## License

Licensed under the [Apache License, Version 2.0](LICENSE).

<div align="center">
<sub>Built with Rust for the Algorand ecosystem.</sub>
</div>
