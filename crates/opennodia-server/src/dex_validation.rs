//! Runtime validation gate for DEX transaction writes.

use std::path::Path;
use std::sync::{Arc, RwLock};

use opennodia_core::{Address, Round};
use opennodia_swap::{
    assign_group_id, build_asset_transfer, build_cancel_group, build_deposit_group,
    build_fill_group, build_payment, derive_lease, encode_dryrun_request, encode_transaction,
    encode_unsigned_signed_tx, render_program, sign_with_logicsig, EscrowAccount, EscrowKind,
    EscrowParams, TransactionFields, TransactionParams, TransactionType, DEFAULT_MAX_FEE,
};
use serde::Serialize;

use crate::state::AppState;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DexValidationPhase {
    Pending,
    Passed,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
pub struct DexValidationSnapshot {
    pub configured_write_enabled: bool,
    pub phase: DexValidationPhase,
    pub checked_at: Option<u64>,
    pub contract_hashes: Vec<String>,
    pub error: Option<String>,
}

impl DexValidationSnapshot {
    pub fn allows_writes(&self) -> bool {
        self.configured_write_enabled && self.phase == DexValidationPhase::Passed
    }
}

#[derive(Clone, Debug)]
pub struct DexValidationRuntime {
    inner: Arc<RwLock<DexValidationSnapshot>>,
}

impl DexValidationRuntime {
    pub fn new(configured_write_enabled: bool) -> Self {
        Self {
            inner: Arc::new(RwLock::new(DexValidationSnapshot {
                configured_write_enabled,
                phase: DexValidationPhase::Pending,
                checked_at: None,
                contract_hashes: Vec::new(),
                error: None,
            })),
        }
    }

    pub fn snapshot(&self) -> DexValidationSnapshot {
        self.inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn record_passed(&self, checked_at: u64, contract_hashes: Vec<String>) {
        let mut state = self
            .inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.phase = DexValidationPhase::Passed;
        state.checked_at = Some(checked_at);
        state.contract_hashes = contract_hashes;
        state.error = None;
    }

    pub fn record_failed(&self, checked_at: u64, error: String) {
        let mut state = self
            .inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.phase = DexValidationPhase::Failed;
        state.checked_at = Some(checked_at);
        state.contract_hashes.clear();
        state.error = Some(error);
    }
}

#[derive(Serialize)]
struct ValidationArtifact {
    checked_at: u64,
    network: String,
    genesis_id: String,
    contracts: Vec<ContractArtifact>,
    codec_group_id: String,
}

#[derive(Serialize)]
struct ContractArtifact {
    name: String,
    address: String,
    program_bytes: usize,
    source_file: String,
}

pub async fn run_startup_validation(state: &AppState) {
    let checked_at = unix_timestamp();
    match validate(state, checked_at).await {
        Ok(hashes) => {
            state
                .runtime
                .dex_validation
                .record_passed(checked_at, hashes.clone());
            tracing::info!(
                configured_write_enabled = state.config.dex.write_enabled,
                contract_count = hashes.len(),
                "DEX runtime validation passed"
            );
        }
        Err(error) => {
            state
                .runtime
                .dex_validation
                .record_failed(checked_at, error.to_string());
            tracing::error!(
                configured_write_enabled = state.config.dex.write_enabled,
                %error,
                "DEX runtime validation failed; transaction writes are disabled"
            );
        }
    }
}

async fn validate(state: &AppState, checked_at: u64) -> anyhow::Result<Vec<String>> {
    let versions = state.ledger.algod.versions().await?;
    if versions.genesis_id != state.config.algod.network.genesis_id() {
        anyhow::bail!(
            "local algod genesis id {} does not match configured network {}",
            versions.genesis_id,
            state.config.algod.network.genesis_id()
        );
    }
    validate_codec_golden_vector()?;

    let status = state.ledger.algod.status().await?;
    let protocol_version = match state.ledger.public_algod.as_ref() {
        Some(public) => public
            .status()
            .await
            .map(|status| status.last_version)
            .unwrap_or_else(|_| status.last_version.clone()),
        None => status.last_version.clone(),
    };
    if protocol_version.is_empty() {
        anyhow::bail!("algod status did not report a consensus protocol version");
    }
    let expire_round = status.last_round.as_u64().saturating_add(10_000);
    let owner = Address::from_bytes([0x11; 32]);
    let filler = Address::from_bytes([0x22; 32]);
    let cases = [
        (
            "algo-for-asa",
            EscrowKind::Buy,
            EscrowParams::new(owner, 0, 500_000, 2, 1_000, expire_round),
        ),
        (
            "asa-for-algo",
            EscrowKind::Sell,
            EscrowParams::new(owner, 1, 1_000, 0, 500_000, expire_round),
        ),
        (
            "asa-for-asa",
            EscrowKind::Sell,
            EscrowParams::new(owner, 1, 1_000, 2, 2_000, expire_round),
        ),
    ];
    let params = TransactionParams {
        fee: 1_000,
        first_valid: status.last_round,
        last_valid: status.last_round + 1_000,
        genesis_id: versions.genesis_id.clone(),
        genesis_hash: decode_genesis_hash(&versions.genesis_hash_b64)?,
    };
    let artifact_dir = state.config.data_dir.join("dex-validation");
    std::fs::create_dir_all(&artifact_dir)?;
    let mut artifacts = Vec::new();
    let mut hashes = Vec::new();
    for (name, kind, escrow_params) in cases {
        let source = render_program(kind, &escrow_params);
        validate_source_guards(&source)?;
        let escrow = EscrowAccount::compile(&state.ledger.algod, kind, escrow_params).await?;
        validate_transaction_builders(&escrow, filler, &params)?;
        validate_dryrun_matrix(
            &state.ledger.algod,
            &escrow,
            filler,
            &params,
            &protocol_version,
        )
        .await?;
        let source_file = format!("{name}.teal");
        std::fs::write(artifact_dir.join(&source_file), source)?;
        hashes.push(escrow.address.to_string());
        artifacts.push(ContractArtifact {
            name: name.to_string(),
            address: escrow.address.to_string(),
            program_bytes: escrow.program.len(),
            source_file,
        });
    }

    let codec_group_id = codec_group_id();
    let artifact = ValidationArtifact {
        checked_at,
        network: state.config.algod.network.to_string(),
        genesis_id: versions.genesis_id,
        contracts: artifacts,
        codec_group_id,
    };
    write_json_atomic(
        &artifact_dir.join("manifest.json"),
        &serde_json::to_vec_pretty(&artifact)?,
    )?;
    Ok(hashes)
}

async fn validate_dryrun_matrix(
    algod: &opennodia_node::AlgodClient,
    escrow: &EscrowAccount,
    filler: Address,
    params: &TransactionParams,
    protocol_version: &str,
) -> anyhow::Result<()> {
    let deposit = build_deposit_group(escrow, params)?;
    let deposit_group = if deposit.logicsig_txs.is_empty() {
        deposit.owner_txs.clone()
    } else {
        vec![
            deposit.owner_txs[0].clone(),
            deposit.logicsig_txs[0].clone(),
            deposit.owner_txs[1].clone(),
        ]
    };
    let deposit_lsig = (!deposit.logicsig_txs.is_empty()).then_some(1);
    dryrun_expect(
        algod,
        escrow,
        &deposit_group,
        &deposit_lsig.into_iter().collect::<Vec<_>>(),
        protocol_version,
        true,
        "create baseline",
    )
    .await?;

    let fill = build_fill_group(escrow, filler, derive_lease(filler, escrow.address), params)?;
    let mut fill_group = vec![fill.filler_tx];
    fill_group.extend(fill.escrow_txs);
    let fill_lsig: Vec<usize> = (1..fill_group.len()).collect();
    dryrun_expect(
        algod,
        escrow,
        &fill_group,
        &fill_lsig,
        protocol_version,
        true,
        "fill baseline",
    )
    .await?;

    let cancel = build_cancel_group(escrow, params)?;
    let mut cancel_group = vec![cancel.owner_auth_tx];
    cancel_group.extend(cancel.escrow_txs);
    let cancel_lsig: Vec<usize> = (1..cancel_group.len()).collect();
    dryrun_expect(
        algod,
        escrow,
        &cancel_group,
        &cancel_lsig,
        protocol_version,
        true,
        "cancel baseline",
    )
    .await?;

    let mut attacks = Vec::new();
    let mut wrong_type = fill_group.clone();
    wrong_type[0].ty = match wrong_type[0].ty {
        TransactionType::Pay => TransactionType::Axfer,
        _ => TransactionType::Pay,
    };
    attacks.push(("transaction type", wrong_type));

    let mut wrong_amount = fill_group.clone();
    if let Some(amount) = wrong_amount[0].amount.as_mut() {
        *amount = amount.saturating_sub(1);
    } else if let Some(amount) = wrong_amount[0].asset_amount.as_mut() {
        *amount = amount.saturating_sub(1);
    }
    attacks.push(("amount", wrong_amount));

    let mut wrong_receiver = fill_group.clone();
    if let Some(receiver) = wrong_receiver[0].receiver.as_mut() {
        *receiver = filler;
    } else if let Some(receiver) = wrong_receiver[0].asset_receiver.as_mut() {
        *receiver = filler;
    }
    attacks.push(("receiver", wrong_receiver));

    let mut no_lease = fill_group.clone();
    no_lease[1].lease = None;
    attacks.push(("lease", no_lease));

    let mut excessive_fee = fill_group.clone();
    excessive_fee[0].fee = escrow.params.max_fee.saturating_add(1);
    attacks.push(("fee cap", excessive_fee));

    let mut rekey = fill_group.clone();
    rekey[0].rekey_to = Some(filler);
    attacks.push(("rekey", rekey));

    let mut expired = fill_group.clone();
    expired[0].first_valid = Round(escrow.params.expire_round.saturating_add(1));
    expired[0].last_valid = expired[0].first_valid + 1_000;
    attacks.push(("expiry", expired));

    if let Some(asset_id) = fill_group[1].xfer_asset {
        let mut wrong_asset = fill_group.clone();
        wrong_asset[1].xfer_asset = Some(asset_id.saturating_add(1));
        attacks.push(("asset id", wrong_asset));
    }

    let mut wrong_close = fill_group.clone();
    if wrong_close[1].asset_close_to.is_some() {
        wrong_close[1].asset_close_to = Some(escrow.params.owner);
    } else if wrong_close[1].close_remainder_to.is_some() {
        wrong_close[1].close_remainder_to = Some(filler);
    }
    attacks.push(("close target", wrong_close));

    if fill_group.len() > 2 {
        let mut wrong_size = fill_group.clone();
        wrong_size.pop();
        attacks.push(("group size", wrong_size));
    }

    for (name, mut group) in attacks {
        assign_group_id(&mut group);
        dryrun_expect(
            algod,
            escrow,
            &group,
            &fill_lsig,
            protocol_version,
            false,
            name,
        )
        .await?;
    }
    Ok(())
}

async fn dryrun_expect(
    algod: &opennodia_node::AlgodClient,
    escrow: &EscrowAccount,
    transactions: &[TransactionFields],
    lsig_indices: &[usize],
    protocol_version: &str,
    should_pass: bool,
    case: &str,
) -> anyhow::Result<()> {
    if lsig_indices.is_empty() {
        return Ok(());
    }
    let signed: Vec<Vec<u8>> = transactions
        .iter()
        .enumerate()
        .map(|(index, transaction)| {
            if lsig_indices.contains(&index) {
                sign_with_logicsig(transaction.clone(), escrow.program.clone())
            } else {
                encode_unsigned_signed_tx(transaction)
            }
        })
        .collect();
    let accounts = synthetic_accounts(escrow);
    let request = encode_dryrun_request(
        &signed,
        &accounts,
        transactions[0].first_valid,
        protocol_version,
    )?;
    let response = algod.dryrun(request).await?;
    let passed = lsig_indices.iter().all(|index| {
        response["txns"][*index]["logic-sig-messages"]
            .as_array()
            .is_some_and(|messages| {
                messages
                    .iter()
                    .any(|message| message.as_str() == Some("PASS"))
            })
    });
    if passed != should_pass {
        anyhow::bail!(
            "DEX dryrun case {case} expected pass={should_pass}, observed pass={passed}: {}",
            response
        );
    }
    Ok(())
}

fn synthetic_accounts(escrow: &EscrowAccount) -> Vec<serde_json::Value> {
    let owner = escrow.params.owner.to_string();
    let filler = Address::from_bytes([0x22; 32]).to_string();
    let asset_ids: Vec<u64> = [escrow.params.sell_asset, escrow.params.buy_asset]
        .into_iter()
        .filter(|asset_id| *asset_id != 0)
        .collect();
    let holdings = |amount: u64| {
        asset_ids
            .iter()
            .map(|asset_id| {
                serde_json::json!({
                    "asset-id": asset_id,
                    "amount": amount,
                    "is-frozen": false
                })
            })
            .collect::<Vec<_>>()
    };
    let created_assets = asset_ids
        .iter()
        .map(|asset_id| {
            serde_json::json!({
                "index": asset_id,
                "params": {
                    "creator": owner,
                    "total": 1_000_000_000u64,
                    "decimals": 0,
                    "default-frozen": false
                }
            })
        })
        .collect::<Vec<_>>();
    let escrow_assets = if escrow.params.sell_asset == 0 {
        Vec::new()
    } else {
        vec![serde_json::json!({
            "asset-id": escrow.params.sell_asset,
            "amount": escrow.params.sell_amount,
            "is-frozen": false
        })]
    };
    let escrow_amount = if escrow.params.sell_asset == 0 {
        escrow
            .params
            .sell_amount
            .saturating_add(opennodia_swap::BASE_ESCROW_FUNDING_MICROALGO)
    } else {
        opennodia_swap::MIN_ESCROW_FUNDING_MICROALGO
    };
    vec![
        serde_json::json!({
            "address": owner,
            "amount": 100_000_000u64,
            "amount-without-pending-rewards": 100_000_000u64,
            "min-balance": 100_000u64,
            "status": "Offline",
            "assets": holdings(100_000_000),
            "created-assets": created_assets
        }),
        serde_json::json!({
            "address": filler,
            "amount": 100_000_000u64,
            "amount-without-pending-rewards": 100_000_000u64,
            "min-balance": 100_000u64,
            "status": "Offline",
            "assets": holdings(100_000_000)
        }),
        serde_json::json!({
            "address": escrow.address.to_string(),
            "amount": escrow_amount,
            "amount-without-pending-rewards": escrow_amount,
            "min-balance": 100_000u64,
            "status": "Offline",
            "assets": escrow_assets
        }),
    ]
}

fn validate_source_guards(source: &str) -> anyhow::Result<()> {
    for required in [
        "RekeyTo",
        "FirstValid",
        "CloseRemainderTo",
        "global ZeroAddress",
    ] {
        if !source.contains(required) {
            anyhow::bail!("generated escrow source is missing {required}");
        }
    }
    if source.contains("txn LastValid") || source.contains("gtxn 0 LastValid") {
        anyhow::bail!("generated escrow source uses LastValid for expiry");
    }
    Ok(())
}

fn validate_transaction_builders(
    escrow: &EscrowAccount,
    filler: Address,
    params: &TransactionParams,
) -> anyhow::Result<()> {
    let deposit = build_deposit_group(escrow, params)?;
    let fill = build_fill_group(escrow, filler, derive_lease(filler, escrow.address), params)?;
    let cancel = build_cancel_group(escrow, params)?;
    for transaction in deposit
        .logicsig_txs
        .iter()
        .chain(fill.escrow_txs.iter())
        .chain(cancel.escrow_txs.iter())
    {
        if transaction.sender != escrow.address {
            anyhow::bail!("LogicSig transaction sender does not match the escrow address");
        }
    }
    for transaction in deposit
        .owner_txs
        .iter()
        .chain(std::iter::once(&cancel.owner_auth_tx))
    {
        if transaction.sender != escrow.params.owner {
            anyhow::bail!("owner transaction sender does not match the escrow owner");
        }
    }
    if fill.filler_tx.sender != filler {
        anyhow::bail!("fill transaction sender does not match the filler");
    }
    for transaction in deposit
        .owner_txs
        .iter()
        .chain(deposit.logicsig_txs.iter())
        .chain(std::iter::once(&fill.filler_tx))
        .chain(fill.escrow_txs.iter())
        .chain(std::iter::once(&cancel.owner_auth_tx))
        .chain(cancel.escrow_txs.iter())
    {
        if transaction.rekey_to.is_some() {
            anyhow::bail!("DEX transaction builder produced a rekey field");
        }
        if transaction.fee > DEFAULT_MAX_FEE {
            anyhow::bail!("DEX transaction builder exceeded the fee cap");
        }
        if encode_transaction(transaction).is_empty() {
            anyhow::bail!("DEX transaction builder produced an empty encoding");
        }
    }
    Ok(())
}

fn validate_codec_golden_vector() -> anyhow::Result<()> {
    let actual = codec_group_id();
    let expected = "db99f9d0a202f19d64d9c6a4c98d6a6e05c37d6f2347b850ccd4821b55bcfb86";
    if actual != expected {
        anyhow::bail!("canonical codec group id mismatch: {actual} != {expected}");
    }
    Ok(())
}

fn codec_group_id() -> String {
    let params = TransactionParams {
        fee: 1_000,
        first_valid: Round(12_345),
        last_valid: Round(13_345),
        genesis_id: "testnet-v1.0".to_string(),
        genesis_hash: [3; 32],
    };
    let sender = Address::from_bytes([1; 32]);
    let receiver = Address::from_bytes([2; 32]);
    let mut transactions = vec![
        build_payment(sender, receiver, 123_456, &params),
        build_asset_transfer(sender, receiver, 42, 77, &params),
    ];
    hex::encode(assign_group_id(&mut transactions))
}

fn decode_genesis_hash(encoded: &str) -> anyhow::Result<[u8; 32]> {
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD.decode(encoded)?;
    decoded
        .try_into()
        .map_err(|bytes: Vec<u8>| anyhow::anyhow!("genesis hash has {} bytes", bytes.len()))
}

fn write_json_atomic(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(temporary, path)?;
    Ok(())
}

fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codec_matches_official_sdk_vector() {
        validate_codec_golden_vector().unwrap();
    }

    #[test]
    fn runtime_gate_requires_configuration_and_validation() {
        let runtime = DexValidationRuntime::new(true);
        assert!(!runtime.snapshot().allows_writes());
        runtime.record_passed(1, vec!["hash".to_string()]);
        assert!(runtime.snapshot().allows_writes());

        let disabled = DexValidationRuntime::new(false);
        disabled.record_passed(1, vec!["hash".to_string()]);
        assert!(!disabled.snapshot().allows_writes());
    }
}
