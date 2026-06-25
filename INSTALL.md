# OpenNodia Installation Guide

OpenNodia is currently a development/Testnet self-hosted stack. Do not run it
with production wallets until the v1 release blockers in `CHANGELOG.md` are
closed and an independent security review has cleared the release.

## Supported targets

| Target | Status | Notes |
|---|---|---|
| Linux x86_64 | Verified development target | Ubuntu 24.04 class hosts with Docker Engine and Compose. |
| Linux ARM64 | Build target | Container images are built for `linux/arm64`; full node sync must be verified on target hardware. |
| macOS | Planned | Use Docker Desktop/Colima. Full install/upgrade/reboot validation is not yet complete. |
| Windows | Planned | Use Docker Desktop with WSL2 or native PowerShell secret initialization. Full validation is not yet complete. |

Measured development host on 2026-06-20:

- Linux `6.17.0-20-generic`, x86_64
- Docker `29.4.0`
- Docker Compose `v5.1.3`
- 32 logical CPUs, 3.6 TB root filesystem

Minimum versions for development:

- Docker Engine 25+
- Docker Compose v2.24+ or Docker Desktop with Compose v2+
- Git 2.40+
- Node.js 22.12+ and npm 10+ for frontend development
- Rust 1.95 for native development

## Hardware requirements

Recommended self-hosted Testnet deployment:

- CPU: 4 cores
- RAM: 8 GB
- Disk: 100 GB SSD
- Network: 10 Mbps

Recommended Mainnet deployment:

- CPU: 8 cores
- RAM: 16 GB
- Disk: 200 GB usable SSD, 256 GB device recommended
- Network: 50 Mbps+

The default bounded Indexer keeps recent history and permanent wallet-selected
history, not full chain history. A high-volume wallet can exceed the normal
disk budget because selected wallet history is retained permanently.

## First install

```bash
git clone https://github.com/AbaloneLabs/OpenNodia.git
cd OpenNodia
./scripts/init-secrets.sh
docker compose up -d
```

Open `http://localhost:30080` or the LAN URL for the host and complete PIN
setup.

For Windows PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\init-secrets.ps1
docker compose up -d
```

## Upgrade

```bash
git pull --ff-only
./scripts/init-secrets.sh
docker compose pull
docker compose up -d --build
```

`init-secrets` is idempotent. It must reuse existing secret files. If an
existing PostgreSQL volume was initialized with an old password, deleting the
secret directory can prevent PostgreSQL from starting.

## Reboot validation

After host reboot:

```bash
docker compose ps
docker compose logs --tail=100 opennodia
docker compose exec postgres pg_isready -U algorand -d indexer
```

The same secret files should be mounted again, and OpenNodia should keep using
the existing algod token and Indexer database password.

## Testnet functional validation

A new installer should be able to:

1. Create or connect a Testnet wallet.
2. Create an order.
3. Fill an order.
4. Cancel an order.
5. Discover a native AMM pool and perform a native pool swap.
6. Preview external route candidates without confusing AMM virtual quotes with
   confirmed orderbook trades.
