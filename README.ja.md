<div align="center">

> **⚠️ 警告：このプロジェクトは現在開発中です。**
> 本番環境でビルドまたは実行しないでください。機能が不完全であり、セキュリティの脆弱性が含まれる可能性があります。
>
> デフォルトのネットワークは **Algorand Testnet** です。無料のテスト用 ALGO は
> [Algorand Testnet ディスペンサー](https://bank.testnet.algorand.network/)から、
> テスト用 USDC は [Circle Testnet Faucet](https://faucet.circle.com/)から入手できます。

<br>

<img src="assets/opennodia-logo.svg" alt="OpenNodia" width="180" height="180"/>

# OpenNodia

**あなたのノード。あなたの資産。あなたの市場。**

Algorand 向けオープンソース・セルフホスティング DEX ノード。

[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Language: Rust](https://img.shields.io/badge/Language-Rust-dea584.svg)](https://www.rust-lang.org/)
[![Status: Pre-Alpha](https://img.shields.io/badge/Status-Pre--Alpha-red.svg)](#ロードマップ)
[![Platform: Algorand](https://img.shields.io/badge/Platform-Algorand-00d4aa.svg)](https://algorand.org/)

<br>

<a href="README.md">English</a> ·
<a href="README.ko.md">한국어</a> ·
<a href="README.zh.md">中文</a> ·
<a href="README.ja.md"><b>日本語</b></a>

<br><br>

</div>

---

## 概要

OpenNodia は、**独自の Algorand ノードを運用**し、自分の台帳コピーで ASA 資産を検証し、自分が管理するセルフホスティングサーバーで**非カストディアル DEX**を運営できるようにします。

中央集権的な取引所も、カストディも、何が「真実」かを決めるサードパーティ API もありません。あるのは、あなたのノード、あなたの資産、あなたのルールだけです。

```
┌──────────────────────────────────────────────────────────────────┐
│                    あなたの PC / NAS / VPS                       │
│                                                                  │
│   ┌──────────────┐         ┌──────────────────────────────────┐  │
│   │              │  読取   │       OpenNodia デーモン          │  │
│   │   Algorand   │◄────────┤   ┌──────────┬───────────────┐   │  │
│   │    ノード    │         │   │ Web UI   │  資産管理      │   │  │
│   │   (algod)    │────────►┤   ├──────────┼───────────────┤   │  │
│   │              │  イベント│   │ PIN 認証 │  DEX エンジン  │   │  │
│   └──────────────┘         │   ├──────────┼───────────────┤   │  │
│                            │   │ Connect  │  SQLite キャッシュ│  │
│                            │   └──────────┴───────────────┘   │  │
│                            └───────────────┬──────────────────┘  │
└────────────────────────────────────────────┼─────────────────────┘
                                             │ HTTP / WebSocket
                          ┌──────────────────┼──────────────────┐
                          ▼                  ▼                  ▼
                    ┌──────────┐       ┌──────────┐       ┌──────────┐
                    │ デスクトップ│      │  モバイル │       │  タブレット│
                    │ ブラウザ  │       │ ブラウザ │       │ ブラウザ │
                    └──────────┘       └──────────┘       └──────────┘
```

## なぜ OpenNodia か？

ほとんどの Algorand ユーザーはホスティングされたエクスプローラーやサードパーティ DEX に依存しています。OpenNodia はそのモデルを逆転させます：

| | 従来の方式 | OpenNodia |
|---|---|---|
| **台帳ソース** | パブリック API（レート制限、サードパーティ） | 自分のノード（ローカル、無制限） |
| **資産カストディ** | 取引所がキーを保持 | 非カストディアル — 常に自分がキーを保持 |
| **DEX** | 中央集権的なマッチングサーバー | セルフホスティング・アトミックスワップ、仲介者なし |
| **信頼** | 「運営者を信頼せよ」 | 「自分のノードを信頼せよ」 |
| **プライバシー** | アカウントデータが外部 API に送信 | クエリはデフォルトでローカルに留まる |

## 主な機能

- **ローカルファースト台帳** — 独自の `algod` ノードから読み取ります。パブリック API はフォールバックに過ぎず、主なソースにはなりません。
- **設計上の非カストディアル** — OpenNodia は資産を保有しません。すべてのトランザクションに明示的な署名が必要です。
- **セルフホスティング DEX** — アトミックスワップで ASA を取引します。中央マッチングエンジンも、制御できないオーダーブックサーバーもありません。
- **ウォレット管理** — Algorand ウォレットの作成またはインポート（kmd ベース）。複数ウォレット、アドレス生成、PIN 保護アクセス。
- **送受信** — 署名前に人間が読めるプレビューで ALGO と ASA を送信。ASA opt-in 対応。
- **PIN 保護の Web アクセス** — 軽量な Web ダッシュボード、PIN で保護（argon2id ハッシュ）。いつでも変更可能。
- **アドレス検証メッセージング** *(計画)* — E2EE で 1:1 チャット、Algorand アドレスで認証。なりすまし不可。
- **AI アシスタント** *(計画)* — OpenNodia の利用を支援するオプションのチャットボットです。お好みの LLM を接続して、ASA ID の検証、資産の説明、質問への回答を依頼できます。読み取りと説明は可能ですが、取引に署名したり売買注文を出したりすることは決してできません。

## コア原則

| 原則 | 意味 |
|---|---|
| **ローカルファースト** | 公共 API ではなく、あなた自身のノードが台帳を検証します。 |
| **非カストディアル** | OpenNodia があなたの資産を保有することはありません。 |
| **セルフホスティング** | 自分の PC、NAS、VPS でデーモンを実行します。 |
| **オープンソース** | Apache-2.0 ライセンス、完全な透明性。 |
| **人間の承認** | すべてのトランザクションにユーザーの明示的な承認が必要です。 |
| **現物資産のみ** | 自由に転送可能な ASA のためのアトミックスワップ。デリバティブなし。 |
| **AI アシスタント** | お好みの LLM を接続するオプションのチャットボット。読み取りと説明は可能ですが、取引に署名したり注文を出したりすることはできません。 |

## コンポーネント

| コンポーネント | 説明 | 状態 |
|---|---|---|
| **OpenNodia Node** | Algorand ノードデーモンおよび台帳コネクタ | :white_check_mark: |
| **OpenNodia Assets** | ASA 資産管理ターミナル（ポリシー等級付き） | :white_check_mark: |
| **OpenNodia DEX** | 非カストディアル、セルフホスティング現物 DEX（アトミックスワップ） | :white_check_mark: |
| **OpenNodia Connect** | アドレス検証済み 1:1 E2EE メッセージング | :construction: |
| **OpenNodia Channels** | 単一パブリッシャーのアナウンスチャンネル | :hourglass: |
| **OpenNodia Mobile** | モバイル Web / PWA コンパニオン | :hourglass: |

## 技術スタック

| レイヤー | 技術 |
|---|---|
| **バックエンド** | [Rust](https://www.rust-lang.org/)（edition 2021、MSRV 1.80） |
| **Web フレームワーク** | [axum](https://github.com/tokio-rs/axum) + [tower-http](https://github.com/tower-rs/tower-http) |
| **ブロックチェーン** | [Algorand](https://algorand.org/)（algod + kmd REST API） |
| **スマートコントラクト** | TEAL v8 LogicSig（algod がソースからコンパイル） |
| **データベース** | SQLite（[rusqlite](https://github.com/rusqlite/rusqlite)）— ローカルオーダーブックとキャッシュ |
| **フロントエンド** | [Svelte](https://svelte.dev/) + [Vite](https://vitejs.dev/) + [Tailwind CSS](https://tailwindcss.com/) |
| **認証** | argon2id（PIN）+ HMAC セッショントークン |
| **アーキテクチャ** | Cargo workspace モノリポジトリ |
| **ライセンス** | Apache-2.0 |

## リポジトリ構成

```
opennodia/
├── crates/
│   ├── opennodia-core/      # 共有型：Address, AssetId, MicroAlgo, Round
│   ├── opennodia-node/      # algod/kmd REST クライアント、ノードステータス、アカウント/資産照会
│   ├── opennodia-assets/    # ASA 管理、ポリシー等級（open/bridged/regulated）
│   ├── opennodia-swap/      # アトミックスワップ：エスクロー、トランザクションビルダー、マッチングエンジン
│   ├── opennodia-dex/       # ローカルオーダーブック：SQLite 永続化、オンチェーンイベント追跡
│   └── opennodia-server/    # HTTP サーバー、Web UI、PIN 認証、ウォレット管理、DEX API
├── frontend/                # Svelte SPA（多言語：EN/KO/ZH/JA）
├── docker/                  # algod コンテナ entrypoint ラッパー
├── Cargo.toml               # ワークスペースルート
├── docker-compose.yml       # ノード、容量制限付き Indexer、PostgreSQL、補助サービス
├── Dockerfile               # マルチステージビルド（フロントエンド + バックエンド）
└── LICENSE                  # Apache-2.0
```

## ロードマップ

| マイルストーン | タイトル | 状態 |
|---|---|:---:|
| **M0** | モノリポジトリスキャフォールド | :white_check_mark: |
| **M1** | ノード & Web サーバー基盤 | :white_check_mark: |
| **M2** | 資産ダッシュボード | :white_check_mark: |
| **M3** | アトミックスワップコア | :white_check_mark: |
| **M4** | ローカルオーダーブック DEX | :white_check_mark: |
| **M5** | コミュニティ DEX | :construction: |
| **M6** | Connect：検証済み DM | :hourglass: |
| **M7** | アナウンスチャンネル | :hourglass: |
| **M8** | ローカル Indexer | :white_check_mark: |
| **M9** | AI エージェントブリッジ | :hourglass: |
| **M10** | モバイル Web & PWA | :hourglass: |
| **M11** | パブリックリリース (v1.0) | :hourglass: |

## ハードウェア要件

OpenNodia は参加ノード、軽量フォロワーノード、Conduit パイプライン、PostgreSQL、
読み取り専用インデクサー API を実行します。以下はセルフホスト構成の推奨スペックです。

### 推奨スペック

| リソース | Testnet | Mainnet |
|----------|---------|---------|
| **CPU** | 4 コア | 8 コア |
| **メモリ** | 8 GB | 16 GB |
| **ディスク** | 100 GB SSD | 使用可能 200 GB SSD（256 GB デバイス推奨） |
| **ネットワーク** | 10 Mbps | 50 Mbps+ |

### 注意事項

- **2 つの algod ノード。** 参加ノードはブロック報酬を獲得しトランザクションを
  リレーし、フォロワーノードは Conduit のための軽量 non-archival データソースです。
  フォロワーはデフォルトで 2,000 ラウンドの account delta のみを保持します。
- **ディスクの種類が重要です。** SSD（または NVMe）を強く推奨します。HDD は
  algod のキャッチアップとインデクサーのブートストラップを非常に遅くします。
- **容量制限付きローカル Indexer。** デフォルトでは直近 20,000 ラウンドのみを保持し、
  transaction、participation、block header を同じ基準で削除します。登録済みウォレットの
  取引は別の PostgreSQL schema に永続保存し、それ以外の古い履歴は public Indexer を使います。
- **Mainnet 容量予算。** 参加ノード、フォロワー、容量制限付き PostgreSQL、
  コンテナの余裕を含めて通常 120–180 GB を見込みます。取引所規模の高頻度ウォレットを
  登録した場合、永続キャッシュがこの見積もりを超える可能性があります。
- **ブロック報酬への影響なし。** 参加ノードはフォロワーノードや Conduit パイプライン
  と完全に独立しています。有効な参加キーがオンラインの 30,000 ALGO 以上のノードは、
  フォロワーやインデクサーの実行状態に関係なくブロック提案と報酬獲得を継続します。

## クイックスタート

> **OpenNodia は積極的に開発中です。** コアスタック（ノードコネクタ、資産
ダッシュボード、ウォレット管理、送受信、ローカル DEX）は Algorand Testnet
で機能します。Docker Compose を使用するのが最速の始め方です。

### 方法 A：Docker Compose（推奨）

```bash
# リポジトリのクローン
git clone https://github.com/AbaloneLabs/OpenNodia.git
cd OpenNodia

# サンプル設定をコピー
cp opennodia.sample.toml opennodia.toml

# リポジトリ外にインストール固有の secret を生成
./scripts/init-secrets.sh
# Windows PowerShell:
# powershell -ExecutionPolicy Bypass -File .\scripts\init-secrets.ps1

# 完全なスタックを起動：
#   algod（参加ノード）+ algod-follower + conduit + postgres + indexer + opennodia
docker compose up -d

# 初期化スクリプトが表示したアドレスで Web UI を開く
# 例：http://192.168.1.20:30080
```

初期化スクリプトは既定経路のプライベート IPv4 を検出し、そのホスト
インターフェースだけに Web UI を公開します。同じ LAN から接続できますが、
`0.0.0.0` のように全インターフェースを公開しません。ホスト内だけに制限する
場合は初期化前に `OPENNODIA_BIND_ADDRESS=127.0.0.1` を設定し、別の
インターフェースを使う場合はその IP を明示してください。ルーティングされた
ネットワークからの最終的な到達範囲は、ホストとネットワークの
ファイアウォール規則に依存します。

`init-secrets.sh` は
`${XDG_CONFIG_HOME:-$HOME/.config}/opennodia/secrets` に、権限を制限した
インストール固有の認証情報を生成します。`.env` には通常設定と secret
ディレクトリの絶対パスだけが残ります。Docker には読み取り専用 secret
ファイルとして渡されるため、通常の Compose 出力、コンテナ確認、Git 操作で
実値は表示されません。初期化スクリプトの再実行やホスト、Docker、コンテナの
再起動でも既存ファイルを再利用し、新しい認証情報は生成しません。明示済みの
`OPENNODIA_BIND_ADDRESS` も維持されます。
セキュリティ境界とエージェント作業規則は
[SECURITY.md](SECURITY.md) を参照してください。

初めて UI を開くとき、PIN を設定します。その後、Algorand ウォレットを作成またはインポートして使い始めることができます。

スタックには軽量 Indexer bootstrap/pruning ヘルパーも含まれます：

| サービス | 役割 |
|----------|------|
| `algod` | 参加ノード — コンセンサス、ブロック報酬、kmd、トランザクションリレー |
| `algod-follower` | 軽量フォロワーノード — Conduit にブロックデータをストリーミング |
| `conduit` | データパイプライン — フォロワーからブロックを読み取り PostgreSQL に書き込み |
| `postgres` | データベース — インデックスされたブロックチェーンデータを保存 |
| `indexer` | 読み取り専用 REST API — アセット検索と取引履歴を提供 |
| `indexer-bootstrap` | follower の最新位置付近で一度だけ初期化 |
| `indexer-pruner` | ローカル Indexer を直近 20,000 ラウンドに制限 |
| `opennodia` | OpenNodia サーバー + Web UI |

参加ノードはフォロワー/Conduit/indexer パイプラインと完全に独立しているため、ブロック報酬や
ノードコンセンサスに**影響を与えません**。ローカルインデクサーのブートストラップ中は、
OpenNodia が自動的にパブリックインデクサーリレーにフォールバックし、検索と履歴がすぐに機能するようにします。

### 方法 B：ソースからビルド

```bash
# バックエンド（Rust 1.88+ が必要）
cargo build --workspace
cargo run --bin opennodia-server -- --config opennodia.toml

# フロントエンド（別ターミナル）
cd frontend
npm install
npm run dev    # http://localhost:5173（/api を :30080 にプロキシ）
```

### テストの実行

```bash
# すべての Rust テスト（オフライン — algod 不要）
cargo test --workspace
```

## 対象外

- ユーザー資産の保管
- 投資助言
- 自動/アルゴリズム取引
- レバレッジ、マージン、先物、デリバティブ
- 証券取引所
- グループチャット
- AI 駆動の取引、注文実行、取引署名
- AI が開始する売買操作

## コントリビュート

本プロジェクトは積極的に開発中です。現在プルリクエストは受け付けていませんが、以下を歓迎します：

- :beetle: **バグ報告** — [issue](https://github.com/AbaloneLabs/OpenNodia/issues) を作成
- :brain: **AI 活用分析** — AI ツールを使用してセキュリティ脆弱性、コード品質の問題、改善の機会を特定した issue を歓迎します。発見内容を明確に説明してください。
- :bulb: **アイデアやフィードバック** — [discussion](https://github.com/AbaloneLabs/OpenNodia/discussions) を開始
- :globe_with_meridians: **翻訳** — 多言語 README の改善に貢献

## ライセンス

[Apache License, Version 2.0](LICENSE) の下でライセンスされています。

<div align="center">
<sub>Rust で Algorand エコシステムのために構築。</sub>
</div>
