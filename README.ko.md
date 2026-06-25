<div align="center">

> **⚠️ 경고: 이 프로젝트는 현재 개발 중입니다.**
> 프로덕션 환경에서 빌드하거나 실행하지 마세요. 기능이 불완전하며 보안 취약점이 있을 수 있습니다.
>
> 기본 네트워크는 **알고랜드 Testnet**입니다. 무료 테스트용 ALGO는
> [알고랜드 Testnet 디스펜서](https://bank.testnet.algorand.network/)에서,
> 테스트용 USDC는 [Circle Testnet Faucet](https://faucet.circle.com/)에서 받을 수 있습니다.

<br>

<img src="assets/opennodia-logo.svg" alt="OpenNodia" width="180" height="180"/>

# OpenNodia

**내 노드에서 자산을 확인하고, 내 시장을 운영하다.**

알고랜드를 위한 오픈소스 셀프호스팅 DEX 노드.

[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Language: Rust](https://img.shields.io/badge/Language-Rust-dea584.svg)](https://www.rust-lang.org/)
[![Status: Pre-Alpha](https://img.shields.io/badge/Status-Pre--Alpha-red.svg)](#로드맵)
[![Platform: Algorand](https://img.shields.io/badge/Platform-Algorand-00d4aa.svg)](https://algorand.org/)

<br>

<a href="README.md">English</a> ·
<a href="README.ko.md"><b>한국어</b></a> ·
<a href="README.zh.md">中文</a> ·
<a href="README.ja.md">日本語</a>

<br><br>

</div>

---

## 개요

OpenNodia는 **직접 알고랜드 노드를 운영**하고, 자신의 장부 사본으로 ASA 자산을 검증하며, 내가 통제하는 셀프호스팅 서버에서 **비수탁 DEX**를 운영할 수 있게 해줍니다.

중앙화된 거래소도, 자산 보관도, 무엇이 "진실"인지 결정하는 제3자 API도 없습니다. 오직 내 노드, 내 자산, 내 규칙.

```
┌──────────────────────────────────────────────────────────────────┐
│                       내 PC / NAS / VPS                          │
│                                                                  │
│   ┌──────────────┐         ┌──────────────────────────────────┐  │
│   │              │  조회   │       OpenNodia 데몬             │  │
│   │   알고랜드   │◄────────┤   ┌──────────┬───────────────┐   │  │
│   │    노드      │         │   │ 웹 UI    │  자산 관리    │   │  │
│   │   (algod)    │────────►┤   ├──────────┼───────────────┤   │  │
│   │              │  이벤트 │   │ PIN 인증 │  DEX 엔진     │   │  │
│   └──────────────┘         │   ├──────────┼───────────────┤   │  │
│                            │   │ Connect  │  SQLite 캐시  │   │  │
│                            │   └──────────┴───────────────┘   │  │
│                            └───────────────┬──────────────────┘  │
└────────────────────────────────────────────┼─────────────────────┘
                                             │ HTTP / WebSocket
                          ┌──────────────────┼──────────────────┐
                          ▼                  ▼                  ▼
                    ┌──────────┐       ┌──────────┐       ┌──────────┐
                    │ 데스크톱 │       │  모바일  │       │  태블릿  │
                    │ 브라우저 │       │ 브라우저 │       │ 브라우저 │
                    └──────────┘       └──────────┘       └──────────┘
```

## 왜 OpenNodia인가?

대부분의 알고랜드 사용자는 호스팅 익스플로러와 제3자 DEX에 의존합니다. OpenNodia는 그 모델을 뒤집습니다:

| | 기존 방식 | OpenNodia |
|---|---|---|
| **장부 소스** | 공개 API (속도 제한, 제3자) | 내 노드 (로컬, 무제한) |
| **자산 보관** | 거래소가 키를 보관 | 비수탁 — 항상 내가 키를 보관 |
| **DEX** | 중앙화된 매칭 서버 | 셀프호스팅 atomic swap, 중개자 없음 |
| **신뢰** | "운영자를 믿어라" | "내 노드를 믿어라" |
| **프라이버시** | 계정 데이터가 외부 API로 전송 | 조회는 기본적으로 로컬에 머무름 |

## 핵심 기능

- **로컬 우선 장부** — 내 `algod` 노드에서 읽습니다. 공개 API는 fallback일 뿐, 주 소스가 되지 않습니다.
- **설계부터 비수탁** — OpenNodia가 자산을 보관하지 않습니다. 모든 거래는 명시적 서명이 필요합니다.
- **셀프호스팅 DEX** — Atomic swap으로 ASA를 거래합니다. 중앙 매칭 엔진도, 내가 통제하지 않는 주문장 서버도 없습니다.
- **지갑 관리** — 알고랜드 지갑 생성 또는 가져오기 (kmd 기반). 다중 지갑, 주소 생성, PIN 보호 접근.
- **송수신** — ALGO와 ASA를 서명 전에 사람이 읽을 수 있는 미리보기와 함께 전송. ASA opt-in 지원.
- **PIN 기반 웹 접속** — 가벼운 웹 대시보드, PIN으로 보호 (argon2id 해시). 언제든 변경 가능.
- **주소 인증 메시징** *(계획)* — E2EE로 1:1 채팅, 알고랜드 주소로 인증. 사칭 불가.
- **AI 어시스턴트** *(계획)* — OpenNodia 사용을 보조하는 선택적 챗봇입니다. 원하는 LLM을 연결해서 ASA ID 검증, 자산 설명, 질문 답변을 요청할 수 있습니다. 읽고 설명할 수는 있지만, 거래에 서명하거나 매수/매도 주문을 넣을 수는 없습니다.

## 핵심 원칙

| 원칙 | 의미 |
|---|---|
| **로컬 우선** | 공개 API가 아닌 내 노드가 장부를 검증합니다. |
| **비수탁** | OpenNodia가 사용자 자산을 보관하지 않습니다. |
| **셀프호스팅** | 자신의 PC, NAS, VPS에서 데몬을 실행합니다. |
| **오픈소스** | Apache-2.0 라이선스, 완전한 투명성. |
| **사용자 승인** | 모든 거래는 사용자의 명시적 승인이 필요합니다. |
| **현물 자산만** | 자유 전송 가능한 ASA를 위한 atomic swap. 파생상품 없음. |
| **AI 어시스턴트** | 원하는 LLM을 연결하는 선택적 챗봇. 읽고 설명할 수는 있지만, 거래에 서명하거나 주문을 넣을 수는 없습니다. |

## 구성 요소

| 구성 요소 | 설명 | 상태 |
|---|---|---|
| **OpenNodia Node** | 알고랜드 노드 데몬 및 장부 커넥터 | :white_check_mark: |
| **OpenNodia Assets** | ASA 자산 관리 터미널 (정책 등급 포함) | :white_check_mark: |
| **OpenNodia DEX** | 비수탁, 셀프호스팅 현물 DEX (atomic swap) | :white_check_mark: |
| **OpenNodia Connect** | 주소 인증 1:1 E2EE 메시징 | :construction: |
| **OpenNodia Channels** | 단일 퍼블리셔 공지 채널 | :hourglass: |
| **OpenNodia Mobile** | 모바일 웹 / PWA 컴패니언 | :hourglass: |

## 기술 스택

| 계층 | 기술 |
|---|---|
| **백엔드** | [Rust](https://www.rust-lang.org/) (edition 2021, MSRV 1.80) |
| **웹 프레임워크** | [axum](https://github.com/tokio-rs/axum) + [tower-http](https://github.com/tower-rs/tower-http) |
| **블록체인** | [Algorand](https://algorand.org/) (algod + kmd REST API) |
| **스마트 컨트랙트** | TEAL v8 LogicSig (algod가 소스에서 컴파일) |
| **데이터베이스** | SQLite ([rusqlite](https://github.com/rusqlite/rusqlite)) — 로컬 주문장 및 캐시 |
| **프론트엔드** | [Svelte](https://svelte.dev/) + [Vite](https://vitejs.dev/) + [Tailwind CSS](https://tailwindcss.com/) |
| **인증** | argon2id (PIN) + HMAC 세션 토큰 |
| **아키텍처** | Cargo workspace 모노레포 |
| **라이선스** | Apache-2.0 |

## 저장소 구조

```
opennodia/
├── crates/
│   ├── opennodia-core/      # 공통 타입: Address, AssetId, MicroAlgo, Round
│   ├── opennodia-node/      # algod/kmd REST 클라이언트, 노드 상태, 계정/자산 조회
│   ├── opennodia-assets/    # ASA 관리, 정책 등급 (open/bridged/regulated)
│   ├── opennodia-swap/      # Atomic swap: 에스크로, 트랜잭션 빌더, 매칭 엔진
│   ├── opennodia-dex/       # 로컬 주문장: SQLite 영속화, 온체인 이벤트 추적
│   └── opennodia-server/    # HTTP 서버, 웹 UI, PIN 인증, 지갑 관리, DEX API
├── frontend/                # Svelte SPA (다국어: EN/KO/ZH/JA)
├── docker/                  # algod 컨테이너 entrypoint 래퍼
├── Cargo.toml               # 워크스페이스 루트
├── docker-compose.yml       # 노드, 제한형 Indexer, PostgreSQL 및 헬퍼 서비스
├── Dockerfile               # 멀티스테이지 빌드 (프론트엔드 + 백엔드)
└── LICENSE                  # Apache-2.0
```

## 로드맵

| 마일스톤 | 제목 | 상태 |
|---|---|:---:|
| **M0** | 모노레포 스캐폴드 | :white_check_mark: |
| **M1** | 노드 및 웹 서버 기반 | :white_check_mark: |
| **M2** | 자산 대시보드 | :white_check_mark: |
| **M3** | Atomic Swap 코어 | :white_check_mark: |
| **M4** | 로컬 주문장 DEX | :white_check_mark: |
| **M5** | 커뮤니티 DEX | :construction: |
| **M6** | Connect: 인증 DM | :hourglass: |
| **M7** | 공지 채널 | :hourglass: |
| **M8** | 로컬 인덱서 | :white_check_mark: |
| **M9** | AI 에이전트 브리지 | :hourglass: |
| **M10** | 모바일 웹 및 PWA | :hourglass: |
| **M11** | 퍼블릭 릴리즈 (v1.0) | :hourglass: |

## 하드웨어 요구사항

OpenNodia는 참여 노드, 경량 팔로워 노드, Conduit 파이프라인, PostgreSQL, 읽기
전용 인덱서 API를 실행합니다. 다음은 셀프 호스팅 구성을 위한 권장 사양입니다.

### 권장 사양

| 자원 | Testnet | Mainnet |
|------|---------|---------|
| **CPU** | 4코어 | 8코어 |
| **RAM** | 8 GB | 16 GB |
| **디스크** | 100 GB SSD | 사용 가능 200 GB SSD (256 GB 장치 권장) |
| **네트워크** | 10 Mbps | 50 Mbps+ |

### 참고사항

- **두 개의 algod 노드.** 참여 노드는 블록 보상을 받고 트랜잭션을 릴레이합니다.
  팔로워는 Conduit용 non-archival 노드이며 account delta 복구 범위를
  2,000라운드로 줄여 운영합니다.
- **디스크 종류가 중요합니다.** SSD(또는 NVMe)를 강력히 권장합니다. HDD는
  algod 캐치업과 인덱서 부트스트랩을 매우 느리게 만듭니다.
- **용량이 제한된 로컬 Indexer.** 기본 DB는 follower 최신 지점 근처에서 시작하며
  최근 20,000라운드만 유지합니다. 더 오래된 transaction, participation,
  block header는 동일한 기준으로 함께 삭제합니다.
- **지갑 거래 영구 보존.** OpenNodia에 등록된 지갑 주소 관련 거래는 pruning 전에
  별도 PostgreSQL 스키마로 복사합니다. 그 외 오래된 내역은 public Indexer를
  사용합니다.
- **팔로워 복구 범위.** 기본적으로 최근 2,000라운드의 account delta를 유지합니다.
  Conduit가 더 오래 중단될 수 있다면 `ALGOD_FOLLOWER_LOOKBACK`을 늘리세요. 보존
  라운드를 늘리면 팔로워 디스크 사용량도 증가합니다.
- **Mainnet 용량 예산.** participation algod, follower, 제한된 PostgreSQL,
  컨테이너 여유 공간을 포함해 일반적으로 약 120–180 GB를 예상합니다. 거래량이
  거래소 수준인 지갑을 등록하면 영구 캐시가 이 예산을 넘을 수 있습니다.
- **블록 보상에 영향 없음.** 참여 노드는 팔로워 노드 및 Conduit 파이프라인과
  완전히 독립적입니다. 유효한 participation key가 온라인 상태인 30,000 ALGO
  이상 노드는 팔로워나 인덱서 실행 여부와 무관하게 블록 제안과 보상을
  계속 수행합니다.

현재 PostgreSQL 및 Docker 사용량은 다음 명령으로 확인할 수 있습니다.

```bash
docker compose exec postgres psql -U algorand -d indexer \
  -c "SELECT pg_size_pretty(pg_database_size(current_database()));"
docker system df -v
```

## 빠른 시작

> **OpenNodia는 활발히 개발 중입니다.** 핵심 스택 (노드 커넥터, 자산 대시보드,
지갑 관리, 송수신, 로컬 DEX)은 알고랜드 Testnet에서 작동합니다. Docker Compose를
사용하면 가장 빠르게 시작할 수 있습니다.

### 방법 A: Docker Compose (권장)

```bash
# 저장소 클론
git clone https://github.com/AbaloneLabs/OpenNodia.git
cd OpenNodia

# 샘플 설정 복사
cp opennodia.sample.toml opennodia.toml

# 저장소 외부에 설치별 고유 secret 생성
./scripts/init-secrets.sh
# Windows PowerShell:
# powershell -ExecutionPolicy Bypass -File .\scripts\init-secrets.ps1

# 전체 스택 시작:
#   algod (참여 노드) + algod-follower + conduit + postgres + indexer + opennodia
docker compose up -d

# 초기화 스크립트가 출력한 주소로 웹 UI 열기
# 예: http://192.168.1.20:30080
```

초기화 스크립트는 기본 경로의 사설 IPv4를 감지하고 해당 호스트
인터페이스에만 웹 UI를 공개합니다. 따라서 같은 LAN의 다른 기기에서 접근할
수 있으면서 `0.0.0.0`처럼 모든 인터페이스를 열지는 않습니다. 호스트에서만
접근하려면 초기화 전에 `OPENNODIA_BIND_ADDRESS=127.0.0.1`로 설정하고, 자동
감지 대신 다른 인터페이스를 사용하려면 해당 IP를 직접 지정하세요. 라우팅된
네트워크의 최종 접근 범위는 호스트 및 네트워크 방화벽 규칙이 결정합니다.

`init-secrets.sh`는
`${XDG_CONFIG_HOME:-$HOME/.config}/opennodia/secrets` 아래에 설치별 고유
자격증명을 제한된 파일 권한으로 생성합니다. `.env`에는 일반 설정과 secret
디렉터리의 절대 경로만 남습니다. Docker에는 읽기 전용 secret 파일로
전달되므로 일반적인 Compose 렌더링, 컨테이너 검사, Git 작업에서 실제 값이
출력되지 않습니다. 보안 경계와 에이전트 작업 규칙은
[SECURITY.md](SECURITY.md)를 참고하세요. 초기화 스크립트를 다시 실행하거나
PC·Docker·컨테이너를 재시작해도 기존 파일을 재사용하며 새 값을 만들지
않습니다. 이미 명시된 `OPENNODIA_BIND_ADDRESS` 값도 그대로 유지합니다.

처음 UI를 열면 PIN을 설정합니다. 그 후 알고랜드 지갑을 생성하거나 가져와서 시작할 수 있습니다.

스택은 핵심 컨테이너와 Indexer bootstrap/pruning 헬퍼를 실행합니다:

| 서비스 | 역할 |
|--------|------|
| `algod` | 참여 노드 — 합의, 블록 보상, kmd, 트랜잭션 릴레이 |
| `algod-follower` | 경량 팔로워 노드 — Conduit에 블록 데이터 스트리밍 |
| `conduit` | 데이터 파이프라인 — 팔로워에서 블록을 읽어 PostgreSQL에 저장 |
| `postgres` | 데이터베이스 — 인덱싱된 블록체인 데이터 저장 |
| `indexer` | 읽기 전용 REST API — 자산 검색 및 거래 내역 서빙 |
| `indexer-bootstrap` | follower 최신 지점 근처에서 한 번만 초기화 |
| `indexer-pruner` | 로컬 Indexer를 최근 20,000라운드로 제한 |
| `opennodia` | OpenNodia 서버 + 웹 UI |

참여 노드는 팔로워/Conduit/indexer 파이프라인과 완전히 독립적이므로 블록 보상이나
노드 합의에 **영향을 주지 않습니다**. 로컬 Indexer가 동기화 중이면 public Indexer로
폴백하고, 동기화 후 최신 내역은 로컬 Indexer를 사용합니다. 등록된 지갑의 거래는 별도
PostgreSQL 스키마에 영구 보존하며, 그 외 오래된 내역은 public Indexer에서 조회합니다.

### 방법 B: 소스에서 빌드

```bash
# 백엔드 (Rust 1.88+ 필요)
cargo build --workspace
cargo run --bin opennodia-server -- --config opennodia.toml

# 프론트엔드 (별도 터미널)
cd frontend
npm install
npm run dev    # http://localhost:5173 (/api를 :30080으로 프록시)
```

### 테스트 실행

```bash
# 모든 Rust 테스트 (오프라인 — algod 불필요)
cargo test --workspace
```

## 지원하지 않는 것

- 사용자 자산 보관
- 투자 자문
- 자동 / 알고리즘 매매
- 레버리지, 마진, 선물, 파생상품
- 증권형 토큰 거래
- 그룹 채팅
- AI 기반 매매, 주문 실행, 거래 서명
- AI가 시작하는 매수/매도 행위

## 기여

이 프로젝트는 활발히 개발 중입니다. 현재 풀 리퀘스트는 받지 않지만, 다음을 환영합니다:

- :beetle: **버그 리포트** — [이슈](https://github.com/AbaloneLabs/OpenNodia/issues) 등록
- :brain: **AI 활용 분석** — AI 도구를 활용하여 보안 취약점, 코드 품질 문제, 개선 기회를 식별한 이슈를 환영합니다. 발견 내용을 명확하게 설명해 주세요.
- :bulb: **아이디어 및 피드백** — [디스커션](https://github.com/AbaloneLabs/OpenNodia/discussions) 시작
- :globe_with_meridians: **번역** — 다국어 README 개선 참여

## 라이선스

[Apache License, Version 2.0](LICENSE)에 따라 라이선스가 부여됩니다.

<div align="center">
<sub>알고랜드 생태계를 위해 Rust로 제작되었습니다.</sub>
</div>
