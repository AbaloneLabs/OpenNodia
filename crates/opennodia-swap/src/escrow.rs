//! Algorand LogicSig escrow contracts compiled by a real algod node.
//!
//! `sell_asset` is always the asset deposited by the order owner and
//! `buy_asset` is always the asset the owner expects from the filler. The
//! order side is presentation metadata and never changes transaction meaning.

use opennodia_core::Address;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512_256};

/// Maximum fee accepted on the user-signed transaction in an escrow group.
pub const DEFAULT_MAX_FEE: u64 = 10_000;
/// Base account minimum balance for an escrow that holds only ALGO.
pub const BASE_ESCROW_FUNDING_MICROALGO: u64 = 100_000;
/// Minimum balance for an escrow opted into one ASA.
pub const MIN_ESCROW_FUNDING_MICROALGO: u64 = 200_000;
/// Maximum LogicSig program length accepted by the Algorand protocol.
pub const MAX_LOGICSIG_PROGRAM_BYTES: usize = 1_000;
/// AVM version used by escrow programs.
pub const TEAL_VERSION: u64 = 8;
/// Domain separator for owner-authorized cancellation groups.
pub const CANCEL_NOTE_PREFIX: &[u8] = b"OpenNodiaCancelV1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EscrowKind {
    Sell,
    Buy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EscrowParams {
    pub owner: Address,
    /// Asset deposited by the owner. `0` means ALGO.
    pub sell_asset: u64,
    pub sell_amount: u64,
    /// Asset paid by the filler. `0` means ALGO.
    pub buy_asset: u64,
    pub buy_amount: u64,
    pub expire_round: u64,
    pub max_fee: u64,
}

impl EscrowParams {
    pub fn new(
        owner: Address,
        sell_asset: u64,
        sell_amount: u64,
        buy_asset: u64,
        buy_amount: u64,
        expire_round: u64,
    ) -> Self {
        Self {
            owner,
            sell_asset,
            sell_amount,
            buy_asset,
            buy_amount,
            expire_round,
            max_fee: DEFAULT_MAX_FEE,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EscrowAccount {
    pub kind: EscrowKind,
    pub params: EscrowParams,
    pub address: Address,
    pub program: Vec<u8>,
}

impl EscrowAccount {
    /// Construct an escrow from program bytes previously returned by algod.
    pub fn from_program(
        kind: EscrowKind,
        params: EscrowParams,
        program: Vec<u8>,
    ) -> opennodia_core::Result<Self> {
        validate_params(kind, &params)?;
        if program.is_empty() {
            return Err(opennodia_core::Error::Other(
                "escrow program is empty".to_string(),
            ));
        }
        if program.len() > MAX_LOGICSIG_PROGRAM_BYTES {
            return Err(opennodia_core::Error::Other(format!(
                "escrow program is {} bytes; LogicSig limit is {MAX_LOGICSIG_PROGRAM_BYTES}",
                program.len()
            )));
        }
        let address = escrow_address(&program);
        Ok(Self {
            kind,
            params,
            address,
            program,
        })
    }

    /// Compile the canonical contract source through the configured local algod.
    pub async fn compile(
        algod: &opennodia_node::AlgodClient,
        kind: EscrowKind,
        params: EscrowParams,
    ) -> opennodia_core::Result<Self> {
        validate_params(kind, &params)?;
        let source = render_program(kind, &params);
        let program = compile_via_algod(algod, &source).await?;
        Self::from_program(kind, params, program)
    }
}

pub fn escrow_address(program: &[u8]) -> Address {
    let mut hasher = Sha512_256::new();
    hasher.update(b"Program");
    hasher.update(program);
    let digest = hasher.finalize();
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&digest);
    Address::from_bytes(bytes)
}

pub fn validate_params(_kind: EscrowKind, params: &EscrowParams) -> opennodia_core::Result<()> {
    if params.owner.is_zero() {
        return Err(opennodia_core::Error::Other(
            "escrow owner must not be the zero address".to_string(),
        ));
    }
    if params.sell_amount == 0 || params.buy_amount == 0 {
        return Err(opennodia_core::Error::Other(
            "escrow amounts must be greater than zero".to_string(),
        ));
    }
    if params.sell_asset == params.buy_asset {
        return Err(opennodia_core::Error::Other(
            "sell and buy assets must differ".to_string(),
        ));
    }
    if params.expire_round == 0 {
        return Err(opennodia_core::Error::Other(
            "escrow expiry must be greater than zero".to_string(),
        ));
    }
    if params.max_fee == 0 {
        return Err(opennodia_core::Error::Other(
            "escrow fee cap must be greater than zero".to_string(),
        ));
    }
    Ok(())
}

/// Build the exact owner authorization note for a cancellation group.
pub fn cancel_note(params: &EscrowParams) -> Vec<u8> {
    format!(
        "{}:{}:{}:{}:{}:{}:{}",
        std::str::from_utf8(CANCEL_NOTE_PREFIX).expect("ASCII domain separator"),
        params.owner,
        params.sell_asset,
        params.sell_amount,
        params.buy_asset,
        params.buy_amount,
        params.expire_round
    )
    .into_bytes()
}

pub fn render_program(_kind: EscrowKind, params: &EscrowParams) -> String {
    if params.sell_asset == 0 {
        render_algo_escrow(params)
    } else {
        render_asa_escrow(params)
    }
}

fn dynamic_payment_checks(params: &EscrowParams, index_slot: u64, receiver_expr: &str) -> String {
    if params.buy_asset == 0 {
        format!(
            r#"load {index_slot}
gtxns TypeEnum
int pay
==
assert
load {index_slot}
gtxns Receiver
{receiver_expr}
==
assert
load {index_slot}
gtxns Amount
int {amount}
==
assert
load {index_slot}
gtxns CloseRemainderTo
global ZeroAddress
==
assert
"#,
            amount = params.buy_amount
        )
    } else {
        format!(
            r#"load {index_slot}
gtxns TypeEnum
int axfer
==
assert
load {index_slot}
gtxns XferAsset
int {asset}
==
assert
load {index_slot}
gtxns AssetReceiver
{receiver_expr}
==
assert
load {index_slot}
gtxns AssetAmount
int {amount}
==
assert
load {index_slot}
gtxns AssetSender
global ZeroAddress
==
assert
load {index_slot}
gtxns AssetCloseTo
global ZeroAddress
==
assert
"#,
            asset = params.buy_asset,
            amount = params.buy_amount
        )
    }
}

fn dynamic_user_tx_checks(params: &EscrowParams, index_slot: u64) -> String {
    // Enforce expiry against `txn FirstValid` rather than `txn LastValid`.
    //
    // `LastValid` is the *upper* bound of the filler's validity window, so with a
    // default 1000-round window an order created with a short `expire_rounds`
    // could never be filled: the filler's `LastValid` would already exceed
    // `expire_round` even when the order is still live.
    //
    // `FirstValid` is the *lower* bound — the round the filler built the group —
    // and is always <= the actual inclusion round. Checking
    // `FirstValid <= expire_round` therefore rejects only genuinely expired orders
    // while remaining independent of the filler's validity-window width.
    //
    // (`global Round` would be ideal but is unavailable in LogicSig mode.)
    format!(
        r#"load {index_slot}
gtxns RekeyTo
global ZeroAddress
==
assert
load {index_slot}
gtxns Fee
int {max_fee}
<=
assert
load {index_slot}
gtxns FirstValid
int {expire_round}
<=
assert
"#,
        max_fee = params.max_fee,
        expire_round = params.expire_round
    )
}

fn render_algo_escrow(params: &EscrowParams) -> String {
    let owner = params.owner;
    let note_hex = hex::encode(cancel_note(params));
    let payment = dynamic_payment_checks(params, 0, &format!("addr {owner}"));
    let user_checks = dynamic_user_tx_checks(params, 0);
    format!(
        r#"#pragma version {version}
txn GroupIndex
int 0
>
assert
txn GroupIndex
int 1
-
store 0
load 0
gtxns Note
byte 0x{note_hex}
==
bnz cancel

{payment}
{user_checks}
load 0
gtxns Sender
txn Receiver
==
assert
txn TypeEnum
int pay
==
assert
txn Amount
int {sell_amount}
==
assert
txn Receiver
load 0
gtxns Sender
==
assert
txn CloseRemainderTo
addr {owner}
==
assert
txn Fee
int 0
==
assert
txn RekeyTo
global ZeroAddress
==
assert
txn FirstValid
int {expire_round}
<=
assert
txn Lease
global ZeroAddress
!=
return

cancel:
global GroupSize
int 2
==
assert
txn GroupIndex
int 1
==
assert
gtxn 0 TypeEnum
int pay
==
assert
gtxn 0 Sender
addr {owner}
==
assert
gtxn 0 Receiver
addr {owner}
==
assert
gtxn 0 Amount
int 0
==
assert
gtxn 0 CloseRemainderTo
global ZeroAddress
==
assert
gtxn 0 RekeyTo
global ZeroAddress
==
assert
txn TypeEnum
int pay
==
assert
txn Amount
int 0
==
assert
txn Receiver
addr {owner}
==
assert
txn CloseRemainderTo
addr {owner}
==
assert
txn Fee
int 0
==
assert
txn RekeyTo
global ZeroAddress
==
return
"#,
        version = TEAL_VERSION,
        sell_amount = params.sell_amount,
        expire_round = params.expire_round,
    )
}

fn render_asa_escrow(params: &EscrowParams) -> String {
    let owner = params.owner;
    let note_hex = hex::encode(cancel_note(params));
    let payment = dynamic_payment_checks(params, 0, &format!("addr {owner}"));
    let user_checks = dynamic_user_tx_checks(params, 0);
    format!(
        r#"#pragma version {version}
txn TypeEnum
int axfer
==
bnz asset_tx
txn TypeEnum
int pay
==
bnz algo_tx
err

asset_tx:
txn AssetAmount
int 0
==
bnz zero_asset_tx
b fill_asset_release

zero_asset_tx:
txn GroupIndex
int 1
==
assert
gtxn 0 Note
byte 0x{note_hex}
==
bnz cancel
b create_opt_in

algo_tx:
txn GroupIndex
int 2
==
bnz maybe_cancel_algo
b fill_algo_close

maybe_cancel_algo:
gtxn 0 Note
byte 0x{note_hex}
==
bnz cancel
b fill_algo_close

create_opt_in:
global GroupSize
int 3
==
assert
gtxn 0 TypeEnum
int pay
==
assert
gtxn 0 Sender
addr {owner}
==
assert
gtxn 0 Receiver
txn Sender
==
assert
gtxn 0 Amount
int {funding}
==
assert
gtxn 0 CloseRemainderTo
global ZeroAddress
==
assert
gtxn 0 RekeyTo
global ZeroAddress
==
assert
gtxn 0 Fee
int {max_fee}
<=
assert
txn TypeEnum
int axfer
==
assert
txn XferAsset
int {sell_asset}
==
assert
txn AssetReceiver
txn Sender
==
assert
txn AssetSender
global ZeroAddress
==
assert
txn AssetCloseTo
global ZeroAddress
==
assert
txn Fee
int 0
==
assert
txn RekeyTo
global ZeroAddress
==
assert
gtxn 2 TypeEnum
int axfer
==
assert
gtxn 2 Sender
addr {owner}
==
assert
gtxn 2 XferAsset
int {sell_asset}
==
assert
gtxn 2 AssetReceiver
txn Sender
==
assert
gtxn 2 AssetAmount
int {sell_amount}
==
assert
gtxn 2 AssetSender
global ZeroAddress
==
assert
gtxn 2 AssetCloseTo
global ZeroAddress
==
assert
gtxn 2 RekeyTo
global ZeroAddress
==
assert
int 1
return

fill_asset_release:
txn GroupIndex
int 0
>
assert
txn GroupIndex
int 1
+
global GroupSize
<
assert
txn GroupIndex
int 1
-
store 0
txn GroupIndex
int 1
+
store 1
{payment}
{user_checks}
load 0
gtxns Sender
txn AssetReceiver
==
assert
txn XferAsset
int {sell_asset}
==
assert
txn AssetAmount
int {sell_amount}
==
assert
txn AssetReceiver
load 0
gtxns Sender
==
assert
txn AssetCloseTo
load 0
gtxns Sender
==
assert
txn AssetSender
global ZeroAddress
==
assert
txn Fee
int 0
==
assert
txn RekeyTo
global ZeroAddress
==
assert
txn FirstValid
int {expire_round}
<=
assert
txn Lease
global ZeroAddress
!=
assert
load 1
gtxns TypeEnum
int pay
==
assert
load 1
gtxns Sender
txn Sender
==
assert
load 1
gtxns Receiver
addr {owner}
==
assert
load 1
gtxns Amount
int 0
==
assert
load 1
gtxns CloseRemainderTo
addr {owner}
==
assert
load 1
gtxns Fee
int 0
==
assert
load 1
gtxns RekeyTo
global ZeroAddress
==
assert
int 1
return

fill_algo_close:
txn GroupIndex
int 1
>
assert
txn GroupIndex
int 1
-
store 1
txn GroupIndex
int 2
-
store 0
load 1
gtxns TypeEnum
int axfer
==
assert
load 1
gtxns Sender
txn Sender
==
assert
load 1
gtxns AssetCloseTo
load 0
gtxns Sender
==
assert
load 1
gtxns Lease
global ZeroAddress
!=
assert
txn Receiver
addr {owner}
==
assert
txn Amount
int 0
==
assert
txn CloseRemainderTo
addr {owner}
==
assert
txn Fee
int 0
==
assert
txn RekeyTo
global ZeroAddress
==
return

cancel:
global GroupSize
int 3
==
assert
gtxn 0 TypeEnum
int pay
==
assert
gtxn 0 Sender
addr {owner}
==
assert
gtxn 0 Receiver
addr {owner}
==
assert
gtxn 0 Amount
int 0
==
assert
gtxn 0 CloseRemainderTo
global ZeroAddress
==
assert
gtxn 0 RekeyTo
global ZeroAddress
==
assert
txn GroupIndex
int 1
==
bnz cancel_asset
txn GroupIndex
int 2
==
bnz cancel_algo
err

cancel_asset:
txn TypeEnum
int axfer
==
assert
txn XferAsset
int {sell_asset}
==
assert
txn AssetAmount
int 0
==
assert
txn AssetReceiver
addr {owner}
==
assert
txn AssetCloseTo
addr {owner}
==
assert
txn AssetSender
global ZeroAddress
==
assert
txn Fee
int 0
==
assert
txn RekeyTo
global ZeroAddress
==
return

cancel_algo:
gtxn 1 TypeEnum
int axfer
==
assert
gtxn 1 Sender
txn Sender
==
assert
gtxn 1 AssetCloseTo
addr {owner}
==
assert
txn TypeEnum
int pay
==
assert
txn Amount
int 0
==
assert
txn Receiver
addr {owner}
==
assert
txn CloseRemainderTo
addr {owner}
==
assert
txn Fee
int 0
==
assert
txn RekeyTo
global ZeroAddress
==
return
"#,
        version = TEAL_VERSION,
        funding = MIN_ESCROW_FUNDING_MICROALGO,
        max_fee = params.max_fee,
        sell_asset = params.sell_asset,
        sell_amount = params.sell_amount,
        expire_round = params.expire_round,
    )
}

async fn compile_via_algod(
    algod: &opennodia_node::AlgodClient,
    source: &str,
) -> opennodia_core::Result<Vec<u8>> {
    let compiled = algod.compile_teal(source.as_bytes()).await?;
    let program = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        compiled.result.as_bytes(),
    )
    .map_err(|error| {
        opennodia_core::Error::Other(format!("decode algod compile result: {error}"))
    })?;
    let expected = escrow_address(&program).to_string();
    if compiled.hash != expected {
        return Err(opennodia_core::Error::Other(format!(
            "algod compile hash mismatch: returned {}, derived {expected}",
            compiled.hash
        )));
    }
    Ok(program)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(sell_asset: u64, buy_asset: u64) -> EscrowParams {
        EscrowParams::new(
            Address::from_bytes([7u8; 32]),
            sell_asset,
            1_000,
            buy_asset,
            2_000,
            100_000,
        )
    }

    #[test]
    fn order_side_does_not_change_contract_source() {
        let params = params(123, 0);
        assert_eq!(
            render_program(EscrowKind::Sell, &params),
            render_program(EscrowKind::Buy, &params)
        );
    }

    #[test]
    fn cancel_note_is_bound_to_order_parameters() {
        let first = cancel_note(&params(123, 0));
        let second = cancel_note(&params(124, 0));
        assert_ne!(first, second);
    }

    /// The expiry guard must use `txn FirstValid` (the filler's earliest valid
    /// round) rather than `txn LastValid` (the filler's validity-window upper
    /// bound). Using `LastValid` makes orders with short `expire_rounds`
    /// unfillable because the filler's `LastValid` already exceeds `expire_round`.
    /// `global Round` is unavailable in LogicSig mode. See the fill-expiry QA finding.
    #[test]
    fn expiry_guard_uses_first_valid_not_last_valid() {
        // ASA-for-ALGO escrow (sell side).
        let asa_source = render_program(EscrowKind::Sell, &params(123, 0));
        assert!(
            asa_source.contains("FirstValid"),
            "ASA escrow must check FirstValid for expiry"
        );
        assert!(
            !asa_source.contains("LastValid"),
            "ASA escrow must not reference LastValid for expiry"
        );

        // ALGO-for-ASA escrow (buy side, sell_asset == 0).
        let algo_source = render_program(EscrowKind::Buy, &params(0, 123));
        assert!(
            algo_source.contains("FirstValid"),
            "ALGO escrow must check FirstValid for expiry"
        );
        assert!(
            !algo_source.contains("LastValid"),
            "ALGO escrow must not reference LastValid for expiry"
        );
    }

    /// Every rendered escrow program must reference the configured `expire_round`
    /// constant so the on-chain guard actually bounds the order lifetime.
    #[test]
    fn rendered_program_embeds_expire_round() {
        let p = params(123, 0);
        let source = render_program(EscrowKind::Sell, &p);
        assert!(
            source.contains(&format!("int {}", p.expire_round)),
            "escrow source must embed expire_round={}",
            p.expire_round
        );
    }

    #[test]
    fn asa_fill_release_pushes_success_before_return() {
        let source = render_program(EscrowKind::Sell, &params(123, 0));
        assert!(source.contains(
            "load 1\ngtxns RekeyTo\nglobal ZeroAddress\n==\nassert\nint 1\nreturn\n\nfill_algo_close:"
        ));
    }
}
