-- Algorand Indexer v3.9.0 PostgreSQL schema.
-- Source: idb/postgres/internal/schema/setup_postgres.sql
-- Upstream commit: e37097c3aec127785e09010b0f42478422f0fae3 (Apache-2.0).

CREATE TABLE IF NOT EXISTS block_header (
  round bigint PRIMARY KEY,
  realtime timestamp without time zone NOT NULL,
  rewardslevel bigint NOT NULL,
  header jsonb NOT NULL
);

CREATE INDEX IF NOT EXISTS block_header_time ON block_header (realtime);

CREATE TABLE IF NOT EXISTS txn (
  round bigint NOT NULL,
  intra integer NOT NULL,
  typeenum smallint NOT NULL,
  asset bigint NOT NULL,
  txid bytea,
  txn jsonb NOT NULL,
  extra jsonb NOT NULL,
  PRIMARY KEY (round, intra)
);

CREATE INDEX IF NOT EXISTS txn_by_tixid ON txn (txid);

CREATE TABLE IF NOT EXISTS txn_participation (
  addr bytea NOT NULL,
  round bigint NOT NULL,
  intra integer NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS txn_participation_i
  ON txn_participation (addr, round DESC, intra DESC);

CREATE TABLE IF NOT EXISTS account (
  addr bytea PRIMARY KEY,
  microalgos bigint NOT NULL,
  rewardsbase bigint NOT NULL,
  rewards_total bigint NOT NULL,
  deleted bool NOT NULL,
  created_at bigint NOT NULL,
  closed_at bigint,
  keytype varchar(8),
  account_data jsonb NOT NULL
);

CREATE TABLE IF NOT EXISTS account_asset (
  addr bytea NOT NULL,
  assetid bigint NOT NULL,
  amount numeric(20) NOT NULL,
  frozen boolean NOT NULL,
  deleted bool NOT NULL,
  created_at bigint NOT NULL,
  closed_at bigint,
  PRIMARY KEY (addr, assetid)
);

CREATE INDEX IF NOT EXISTS account_asset_by_addr_partial
  ON account_asset(addr) WHERE NOT deleted;

CREATE TABLE IF NOT EXISTS asset (
  index bigint PRIMARY KEY,
  creator_addr bytea NOT NULL,
  params jsonb NOT NULL,
  deleted bool NOT NULL,
  created_at bigint NOT NULL,
  closed_at bigint
);

CREATE INDEX IF NOT EXISTS asset_by_creator_addr_deleted
  ON asset(creator_addr, deleted);

CREATE TABLE IF NOT EXISTS metastate (
  k text PRIMARY KEY,
  v jsonb
);

CREATE TABLE IF NOT EXISTS app (
  index bigint PRIMARY KEY,
  creator bytea NOT NULL,
  params jsonb NOT NULL,
  deleted bool NOT NULL,
  created_at bigint NOT NULL,
  closed_at bigint
);

CREATE INDEX IF NOT EXISTS app_by_creator_deleted
  ON app(creator, deleted);

CREATE TABLE IF NOT EXISTS account_app (
  addr bytea,
  app bigint,
  localstate jsonb NOT NULL,
  deleted bool NOT NULL,
  created_at bigint NOT NULL,
  closed_at bigint,
  PRIMARY KEY (addr, app)
);

CREATE INDEX IF NOT EXISTS account_app_by_addr_partial
  ON account_app(addr) WHERE NOT deleted;

CREATE TABLE IF NOT EXISTS app_box (
  app bigint NOT NULL,
  name bytea NOT NULL,
  value bytea NOT NULL,
  PRIMARY KEY (app, name)
);
