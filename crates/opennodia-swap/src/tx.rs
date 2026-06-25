//! Algorand transaction construction, msgpack encoding, and group assembly.
//!
//! This module implements a lightweight Algorand transaction encoder following
//! the canonical msgpack field format used by go-algorand. We avoid a heavy SDK
//! dependency by hand-encoding the well-defined transaction fields.
//!
//! # Field reference (msgpack keys)
//!
//! See <https://github.com/algorand/go-algorand/blob/master/data/transactions/transaction.go>
//!
//! | key    | field              | type        |
//! |--------|--------------------|-------------|
//! | `fee`  | Fee                | `uint64`    |
//! | `fv`   | FirstValid         | `uint64`    |
//! | `gen`  | GenesisID          | `string`    |
//! | `gh`   | GenesisHash        | `[32]byte`  |
//! | `lv`   | LastValid          | `uint64`    |
//! | `note` | Note               | `[]byte`    |
//! | `snd`  | Sender             | `[32]byte`  |
//! | `type` | Type               | `string`    |
//! | `xaid` | XferAsset          | `uint64`    |
//! | `aamt` | AssetAmount        | `uint64`    |
//! | `arcv` | AssetReceiver      | `[32]byte`  |
//! | `asnd` | AssetSender        | `[32]byte`  |
//! | `aclose` | AssetCloseTo     | `[32]byte`  |
//! | `amt`  | Amount             | `uint64`    |
//! | `rcv`  | Receiver           | `[32]byte`  |
//! | `close`| CloseRemainderTo   | `[32]byte`  |
//! | `fadd` | FreezeAccount      | `[32]byte`  |
//! | `faid` | FreezeAsset        | `uint64`    |
//! | `afrz` | AssetFrozen        | `bool`      |
//! | `grp`  | Group              | `[32]byte`  |
//! | `lx`   | Lease              | `[32]byte`  |
//! | `rekey`| RekeyTo            | `[32]byte`  |
//! | `apbx` | ApplicationBoxes   | `[]BoxRef`  |

use base64::Engine;
use opennodia_core::{Address, Round};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512_256};

/// Algorand transaction type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionType {
    Pay,
    Axfer,
    Acfg,
    Afrz,
    Appl,
    Keyreg,
}

impl TransactionType {
    /// The canonical type string used in the `type` msgpack field.
    pub fn as_str(self) -> &'static str {
        match self {
            TransactionType::Pay => "pay",
            TransactionType::Axfer => "axfer",
            TransactionType::Acfg => "acfg",
            TransactionType::Afrz => "afrz",
            TransactionType::Appl => "appl",
            TransactionType::Keyreg => "keyreg",
        }
    }
}

/// Suggested transaction parameters fetched from algod.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransactionParams {
    /// Flat transaction fee in microAlgos.
    pub fee: u64,
    /// First valid round.
    pub first_valid: Round,
    /// Last valid round (must be >= first_valid + 1000 for safety).
    pub last_valid: Round,
    /// Genesis ID string (e.g. "testnet-v1.0").
    pub genesis_id: String,
    /// Genesis hash (32 bytes).
    pub genesis_hash: [u8; 32],
}

impl TransactionParams {
    /// Build params with a 1000-round validity window from `first_valid`.
    pub fn new(first_valid: Round, genesis_id: String, genesis_hash: [u8; 32]) -> Self {
        Self {
            fee: 0,
            first_valid,
            last_valid: first_valid + 1000,
            genesis_id,
            genesis_hash,
        }
    }
}

/// All fields of an unsigned Algorand transaction.
///
/// Only set fields are encoded (None → omitted from msgpack).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransactionFields {
    pub ty: TransactionType,
    pub sender: Address,
    pub fee: u64,
    pub first_valid: Round,
    pub last_valid: Round,
    pub genesis_id: Option<String>,
    pub genesis_hash: Option<[u8; 32]>,
    pub note: Option<Vec<u8>>,
    pub lease: Option<[u8; 32]>,
    pub receiver: Option<Address>,
    pub amount: Option<u64>,
    pub close_remainder_to: Option<Address>,
    pub rekey_to: Option<Address>,
    // Asset transfer fields
    pub xfer_asset: Option<u64>,
    pub asset_amount: Option<u64>,
    pub asset_sender: Option<Address>,
    pub asset_receiver: Option<Address>,
    pub asset_close_to: Option<Address>,
    // Asset configuration fields
    pub asset_config_id: Option<u64>,
    pub asset_params: Option<AssetCreateParams>,
    // Application call fields
    pub app_id: Option<u64>,
    pub on_completion: Option<OnCompletion>,
    pub app_args: Vec<Vec<u8>>,
    pub app_accounts: Vec<Address>,
    pub foreign_assets: Vec<u64>,
    pub foreign_apps: Vec<u64>,
    pub boxes: Vec<BoxReference>,
    pub local_state_schema: Option<StateSchema>,
    pub global_state_schema: Option<StateSchema>,
    pub approval_program: Option<Vec<u8>>,
    pub clear_state_program: Option<Vec<u8>>,
    pub extra_program_pages: Option<u32>,
    // Group
    pub group: Option<[u8; 32]>,
}

/// Application box reference.
///
/// `app_index` follows Algorand's foreign application index convention:
/// 0 refers to the called application, non-zero values refer to entries in
/// `foreign_apps`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoxReference {
    pub app_index: u64,
    pub name: Vec<u8>,
}

/// Asset parameters embedded in an asset creation transaction.
///
/// Field names mirror go-algorand's `basics.AssetParams` codec keys:
/// `t`, `dc`, `df`, `un`, `an`, `au`, `am`, `m`, `r`, `f`, `c`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssetCreateParams {
    pub total: u64,
    pub decimals: u32,
    pub default_frozen: bool,
    pub unit_name: String,
    pub asset_name: String,
    pub url: String,
    pub metadata_hash: Option<[u8; 32]>,
    pub manager: Option<Address>,
    pub reserve: Option<Address>,
    pub freeze: Option<Address>,
    pub clawback: Option<Address>,
}

/// Application call OnCompletion value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum OnCompletion {
    NoOp = 0,
    OptIn = 1,
    CloseOut = 2,
    ClearState = 3,
    UpdateApplication = 4,
    DeleteApplication = 5,
}

impl OnCompletion {
    fn as_u64(self) -> u64 {
        self as u64
    }
}

/// Application local/global state schema.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StateSchema {
    pub num_uint: u64,
    pub num_byte_slice: u64,
}

impl StateSchema {
    pub const fn new(num_uint: u64, num_byte_slice: u64) -> Self {
        Self {
            num_uint,
            num_byte_slice,
        }
    }

    fn is_empty(self) -> bool {
        self.num_uint == 0 && self.num_byte_slice == 0
    }
}

/// Configurable fields for a generic application call transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplicationCallFields {
    pub on_completion: OnCompletion,
    pub app_args: Vec<Vec<u8>>,
    pub app_accounts: Vec<Address>,
    pub foreign_assets: Vec<u64>,
    pub foreign_apps: Vec<u64>,
    pub boxes: Vec<BoxReference>,
}

impl ApplicationCallFields {
    pub fn no_op(app_args: Vec<Vec<u8>>) -> Self {
        Self {
            on_completion: OnCompletion::NoOp,
            app_args,
            app_accounts: Vec::new(),
            foreign_assets: Vec::new(),
            foreign_apps: Vec::new(),
            boxes: Vec::new(),
        }
    }
}

impl TransactionFields {
    /// Create a minimal transaction with the given type and params.
    pub(crate) fn base(ty: TransactionType, sender: Address, params: &TransactionParams) -> Self {
        Self {
            ty,
            sender,
            fee: params.fee,
            first_valid: params.first_valid,
            last_valid: params.last_valid,
            genesis_id: Some(params.genesis_id.clone()),
            genesis_hash: Some(params.genesis_hash),
            note: None,
            lease: None,
            receiver: None,
            amount: None,
            close_remainder_to: None,
            rekey_to: None,
            xfer_asset: None,
            asset_amount: None,
            asset_sender: None,
            asset_receiver: None,
            asset_close_to: None,
            asset_config_id: None,
            asset_params: None,
            app_id: None,
            on_completion: None,
            app_args: Vec::new(),
            app_accounts: Vec::new(),
            foreign_assets: Vec::new(),
            foreign_apps: Vec::new(),
            boxes: Vec::new(),
            local_state_schema: None,
            global_state_schema: None,
            approval_program: None,
            clear_state_program: None,
            extra_program_pages: None,
            group: None,
        }
    }
}

/// Build a payment transaction (ALGO transfer).
pub fn build_payment(
    sender: Address,
    receiver: Address,
    amount: u64,
    params: &TransactionParams,
) -> TransactionFields {
    let mut tx = TransactionFields::base(TransactionType::Pay, sender, params);
    tx.receiver = Some(receiver);
    tx.amount = Some(amount);
    tx
}

/// Build an asset transfer transaction (ASA transfer).
pub fn build_asset_transfer(
    sender: Address,
    receiver: Address,
    asset_id: u64,
    amount: u64,
    params: &TransactionParams,
) -> TransactionFields {
    let mut tx = TransactionFields::base(TransactionType::Axfer, sender, params);
    tx.xfer_asset = Some(asset_id);
    tx.asset_amount = Some(amount);
    tx.asset_receiver = Some(receiver);
    tx
}

/// Build an asset opt-in transaction (sender == receiver, amount 0).
pub fn build_asset_opt_in(
    account: Address,
    asset_id: u64,
    params: &TransactionParams,
) -> TransactionFields {
    let mut tx = TransactionFields::base(TransactionType::Axfer, account, params);
    tx.xfer_asset = Some(asset_id);
    tx.asset_amount = Some(0);
    tx.asset_receiver = Some(account);
    tx
}

/// Build an asset creation transaction.
pub fn build_asset_create(
    creator: Address,
    asset_params: AssetCreateParams,
    params: &TransactionParams,
) -> TransactionFields {
    let mut tx = TransactionFields::base(TransactionType::Acfg, creator, params);
    tx.asset_params = Some(asset_params);
    tx
}

/// Build an asset reconfiguration transaction.
pub fn build_asset_config(
    manager: Address,
    asset_id: u64,
    asset_params: AssetCreateParams,
    params: &TransactionParams,
) -> TransactionFields {
    let mut tx = TransactionFields::base(TransactionType::Acfg, manager, params);
    tx.asset_config_id = Some(asset_id);
    tx.asset_params = Some(asset_params);
    tx
}

/// Build an application creation transaction.
pub fn build_application_create(
    sender: Address,
    approval_program: Vec<u8>,
    clear_state_program: Vec<u8>,
    global_state_schema: StateSchema,
    local_state_schema: StateSchema,
    app_args: Vec<Vec<u8>>,
    params: &TransactionParams,
) -> TransactionFields {
    let mut tx = TransactionFields::base(TransactionType::Appl, sender, params);
    tx.on_completion = Some(OnCompletion::NoOp);
    tx.approval_program = Some(approval_program);
    tx.clear_state_program = Some(clear_state_program);
    tx.global_state_schema = Some(global_state_schema);
    tx.local_state_schema = Some(local_state_schema);
    tx.app_args = app_args;
    tx
}

/// Build a generic application call transaction.
pub fn build_application(
    sender: Address,
    app_id: u64,
    fields: ApplicationCallFields,
    params: &TransactionParams,
) -> TransactionFields {
    let mut tx = TransactionFields::base(TransactionType::Appl, sender, params);
    tx.app_id = Some(app_id);
    tx.on_completion = Some(fields.on_completion);
    tx.app_args = fields.app_args;
    tx.app_accounts = fields.app_accounts;
    tx.foreign_assets = fields.foreign_assets;
    tx.foreign_apps = fields.foreign_apps;
    tx.boxes = fields.boxes;
    tx
}

/// Build an application no-op call transaction.
pub fn build_application_call(
    sender: Address,
    app_id: u64,
    app_args: Vec<Vec<u8>>,
    app_accounts: Vec<Address>,
    foreign_assets: Vec<u64>,
    foreign_apps: Vec<u64>,
    params: &TransactionParams,
) -> TransactionFields {
    build_application(
        sender,
        app_id,
        ApplicationCallFields {
            on_completion: OnCompletion::NoOp,
            app_args,
            app_accounts,
            foreign_assets,
            foreign_apps,
            boxes: Vec::new(),
        },
        params,
    )
}

/// Build an application opt-in transaction.
pub fn build_application_opt_in(
    sender: Address,
    app_id: u64,
    app_args: Vec<Vec<u8>>,
    app_accounts: Vec<Address>,
    foreign_assets: Vec<u64>,
    foreign_apps: Vec<u64>,
    params: &TransactionParams,
) -> TransactionFields {
    build_application(
        sender,
        app_id,
        ApplicationCallFields {
            on_completion: OnCompletion::OptIn,
            app_args,
            app_accounts,
            foreign_assets,
            foreign_apps,
            boxes: Vec::new(),
        },
        params,
    )
}

// ============================================================================
// LogicSig + SignedTransaction
// ============================================================================

/// A 64-byte ed25519 signature, stored as a fixed array but (de)serialized
/// as a byte sequence (serde does not derive for arrays > 32 elements).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sig64(pub [u8; 64]);

impl Serialize for Sig64 {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        serde::Serialize::serialize(&self.0.to_vec(), s)
    }
}

impl<'de> Deserialize<'de> for Sig64 {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let bytes: Vec<u8> = serde::Deserialize::deserialize(d)?;
        if bytes.len() != 64 {
            return Err(serde::de::Error::custom(format!(
                "expected 64-byte signature, got {}",
                bytes.len()
            )));
        }
        let mut arr = [0u8; 64];
        arr.copy_from_slice(&bytes);
        Ok(Sig64(arr))
    }
}

/// A LogicSig signature object.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogicSig {
    /// Program bytes (with embedded parameters).
    pub logic: Vec<u8>,
    /// Runtime arguments (empty for template-embedded escrows).
    #[serde(default)]
    pub args: Vec<Vec<u8>>,
    /// Delegated signature (None for program-only LogicSig).
    #[serde(default)]
    pub sig: Option<Sig64>,
}

impl LogicSig {
    /// Create a program-only LogicSig (no delegation signature).
    pub fn from_program(program: Vec<u8>) -> Self {
        Self {
            logic: program,
            args: Vec::new(),
            sig: None,
        }
    }
}

/// A signed transaction: the unsigned transaction + a signature variant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedTransaction {
    pub transaction: TransactionFields,
    /// ed25519 signature (for regular accounts).
    pub sig: Option<Sig64>,
    /// LogicSig signature (for escrows).
    pub lsig: Option<LogicSig>,
}

impl SignedTransaction {
    /// Wrap a transaction with a LogicSig (program-only, no delegation).
    pub fn with_logicsig(transaction: TransactionFields, program: Vec<u8>) -> Self {
        Self {
            transaction,
            sig: None,
            lsig: Some(LogicSig::from_program(program)),
        }
    }
}

// ============================================================================
// msgpack encoding
// ============================================================================

/// A minimal msgpack map writer that produces canonical Algorand transaction
/// encoding. Keys are sorted canonically (by length then lexicographically, as
/// go-algorand's canonical msgpack requires).
struct MsgPack {
    buf: Vec<u8>,
}

impl MsgPack {
    fn new() -> Self {
        Self { buf: Vec::new() }
    }

    fn write_str(&mut self, s: &str) {
        let bytes = s.as_bytes();
        let len = bytes.len();
        // fixstr (0xa0..0xbf) for len <= 31, else str8/str16.
        if len <= 31 {
            self.buf.push(0xa0 | len as u8);
        } else if len <= 0xff {
            self.buf.push(0xd9);
            self.buf.push(len as u8);
        } else {
            self.buf.push(0xda);
            self.buf.extend_from_slice(&(len as u16).to_be_bytes());
        }
        self.buf.extend_from_slice(bytes);
    }

    fn write_bin(&mut self, b: &[u8]) {
        let len = b.len();
        if len <= 0xff {
            self.buf.push(0xc4);
            self.buf.push(len as u8);
        } else if len <= 0xffff {
            self.buf.push(0xc5);
            self.buf.extend_from_slice(&(len as u16).to_be_bytes());
        } else {
            self.buf.push(0xc6);
            self.buf.extend_from_slice(&(len as u32).to_be_bytes());
        }
        self.buf.extend_from_slice(b);
    }

    fn write_u64(&mut self, v: u64) {
        if v <= 0x7f {
            self.buf.push(v as u8); // positive fixint
        } else if v <= 0xff {
            self.buf.push(0xcc);
            self.buf.push(v as u8);
        } else if v <= 0xffff {
            self.buf.push(0xcd);
            self.buf.extend_from_slice(&(v as u16).to_be_bytes());
        } else if v <= 0xffff_ffff {
            self.buf.push(0xce);
            self.buf.extend_from_slice(&(v as u32).to_be_bytes());
        } else {
            self.buf.push(0xcf);
            self.buf.extend_from_slice(&v.to_be_bytes());
        }
    }

    fn write_bool(&mut self, value: bool) {
        self.buf.push(if value { 0xc3 } else { 0xc2 });
    }

    fn write_nil(&mut self) {
        self.buf.push(0xc0);
    }

    fn write_raw(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Begin a map with `n` entries (fixmap/map16/map32).
    fn write_map_header(&mut self, n: usize) {
        if n <= 15 {
            self.buf.push(0x80 | n as u8);
        } else if n <= 0xffff {
            self.buf.push(0xde);
            self.buf.extend_from_slice(&(n as u16).to_be_bytes());
        } else {
            self.buf.push(0xdf);
            self.buf.extend_from_slice(&(n as u32).to_be_bytes());
        }
    }

    /// Begin an array with `n` entries.
    fn write_array_header(&mut self, n: usize) {
        if n <= 15 {
            self.buf.push(0x90 | n as u8);
        } else if n <= 0xffff {
            self.buf.push(0xdc);
            self.buf.extend_from_slice(&(n as u16).to_be_bytes());
        } else {
            self.buf.push(0xdd);
            self.buf.extend_from_slice(&(n as u32).to_be_bytes());
        }
    }

    fn write_json(&mut self, value: &serde_json::Value) -> opennodia_core::Result<()> {
        match value {
            serde_json::Value::Null => self.write_nil(),
            serde_json::Value::Bool(value) => self.write_bool(*value),
            serde_json::Value::Number(value) => {
                let number = value.as_u64().ok_or_else(|| {
                    opennodia_core::Error::Other(
                        "dryrun msgpack only supports non-negative integers".to_string(),
                    )
                })?;
                self.write_u64(number);
            }
            serde_json::Value::String(value) => self.write_str(value),
            serde_json::Value::Array(values) => {
                self.write_array_header(values.len());
                for value in values {
                    self.write_json(value)?;
                }
            }
            serde_json::Value::Object(values) => {
                self.write_map_header(values.len());
                for (key, value) in values {
                    self.write_str(key);
                    self.write_json(value)?;
                }
            }
        }
        Ok(())
    }
}

/// Encode an unsigned transaction to canonical Algorand msgpack.
///
/// Field keys are written in bytewise lexicographic order, matching
/// go-algorand's generated canonical codec.
pub fn encode_transaction(tx: &TransactionFields) -> Vec<u8> {
    let mut m = MsgPack::new();

    // Collect (key, encoded_value_bytes) pairs, then sort canonically and emit.
    let mut entries: Vec<(&'static str, Vec<u8>)> = Vec::new();

    let mut val = MsgPack::new();
    val.write_str(tx.ty.as_str());
    entries.push(("type", std::mem::take(&mut val.buf)));

    if tx.sender != Address::zero() {
        val = MsgPack::new();
        val.write_bin(tx.sender.as_bytes());
        entries.push(("snd", std::mem::take(&mut val.buf)));
    }

    if tx.fee != 0 {
        val = MsgPack::new();
        val.write_u64(tx.fee);
        entries.push(("fee", std::mem::take(&mut val.buf)));
    }

    if tx.first_valid.as_u64() != 0 {
        val = MsgPack::new();
        val.write_u64(tx.first_valid.as_u64());
        entries.push(("fv", std::mem::take(&mut val.buf)));
    }

    if tx.last_valid.as_u64() != 0 {
        val = MsgPack::new();
        val.write_u64(tx.last_valid.as_u64());
        entries.push(("lv", std::mem::take(&mut val.buf)));
    }

    if let Some(gid) = tx.genesis_id.as_ref().filter(|value| !value.is_empty()) {
        val = MsgPack::new();
        val.write_str(gid);
        entries.push(("gen", std::mem::take(&mut val.buf)));
    }
    if let Some(gh) = &tx.genesis_hash.filter(|value| *value != [0u8; 32]) {
        val = MsgPack::new();
        val.write_bin(gh);
        entries.push(("gh", std::mem::take(&mut val.buf)));
    }
    if let Some(note) = &tx.note.as_ref().filter(|value| !value.is_empty()) {
        val = MsgPack::new();
        val.write_bin(note);
        entries.push(("note", std::mem::take(&mut val.buf)));
    }
    if let Some(lease) = &tx.lease.filter(|value| *value != [0u8; 32]) {
        val = MsgPack::new();
        val.write_bin(lease);
        entries.push(("lx", std::mem::take(&mut val.buf)));
    }
    if let Some(rcv) = tx.receiver.filter(|value| *value != Address::zero()) {
        val = MsgPack::new();
        val.write_bin(rcv.as_bytes());
        entries.push(("rcv", std::mem::take(&mut val.buf)));
    }
    if let Some(amt) = tx.amount.filter(|value| *value != 0) {
        val = MsgPack::new();
        val.write_u64(amt);
        entries.push(("amt", std::mem::take(&mut val.buf)));
    }
    if let Some(close) = tx
        .close_remainder_to
        .filter(|value| *value != Address::zero())
    {
        val = MsgPack::new();
        val.write_bin(close.as_bytes());
        entries.push(("close", std::mem::take(&mut val.buf)));
    }
    if let Some(rekey) = tx.rekey_to.filter(|value| *value != Address::zero()) {
        val = MsgPack::new();
        val.write_bin(rekey.as_bytes());
        entries.push(("rekey", std::mem::take(&mut val.buf)));
    }
    if let Some(xaid) = tx.xfer_asset.filter(|value| *value != 0) {
        val = MsgPack::new();
        val.write_u64(xaid);
        entries.push(("xaid", std::mem::take(&mut val.buf)));
    }
    if let Some(aamt) = tx.asset_amount.filter(|value| *value != 0) {
        val = MsgPack::new();
        val.write_u64(aamt);
        entries.push(("aamt", std::mem::take(&mut val.buf)));
    }
    if let Some(asnd) = tx.asset_sender.filter(|value| *value != Address::zero()) {
        val = MsgPack::new();
        val.write_bin(asnd.as_bytes());
        entries.push(("asnd", std::mem::take(&mut val.buf)));
    }
    if let Some(arcv) = tx.asset_receiver.filter(|value| *value != Address::zero()) {
        val = MsgPack::new();
        val.write_bin(arcv.as_bytes());
        entries.push(("arcv", std::mem::take(&mut val.buf)));
    }
    if let Some(aclose) = tx.asset_close_to.filter(|value| *value != Address::zero()) {
        val = MsgPack::new();
        val.write_bin(aclose.as_bytes());
        entries.push(("aclose", std::mem::take(&mut val.buf)));
    }
    if let Some(asset_params) = &tx.asset_params {
        entries.push((
            "apar",
            encode_asset_params(asset_params, tx.asset_config_id.is_some()),
        ));
    }
    if let Some(caid) = tx.asset_config_id.filter(|value| *value != 0) {
        val = MsgPack::new();
        val.write_u64(caid);
        entries.push(("caid", std::mem::take(&mut val.buf)));
    }
    if let Some(app_id) = tx.app_id.filter(|value| *value != 0) {
        val = MsgPack::new();
        val.write_u64(app_id);
        entries.push(("apid", std::mem::take(&mut val.buf)));
    }
    if let Some(on_completion) = tx
        .on_completion
        .filter(|value| *value != OnCompletion::NoOp)
    {
        val = MsgPack::new();
        val.write_u64(on_completion.as_u64());
        entries.push(("apan", std::mem::take(&mut val.buf)));
    }
    if !tx.app_args.is_empty() {
        entries.push(("apaa", encode_bytes_array(&tx.app_args)));
    }
    if !tx.app_accounts.is_empty() {
        entries.push(("apat", encode_address_array(&tx.app_accounts)));
    }
    if !tx.foreign_assets.is_empty() {
        entries.push(("apas", encode_u64_array(&tx.foreign_assets)));
    }
    if !tx.foreign_apps.is_empty() {
        entries.push(("apfa", encode_u64_array(&tx.foreign_apps)));
    }
    if !tx.boxes.is_empty() {
        entries.push(("apbx", encode_box_refs(&tx.boxes)));
    }
    if let Some(schema) = tx
        .global_state_schema
        .filter(|schema| !StateSchema::is_empty(*schema))
    {
        entries.push(("apgs", encode_state_schema(schema)));
    }
    if let Some(schema) = tx
        .local_state_schema
        .filter(|schema| !StateSchema::is_empty(*schema))
    {
        entries.push(("apls", encode_state_schema(schema)));
    }
    if let Some(program) = tx
        .approval_program
        .as_ref()
        .filter(|value| !value.is_empty())
    {
        val = MsgPack::new();
        val.write_bin(program);
        entries.push(("apap", std::mem::take(&mut val.buf)));
    }
    if let Some(program) = tx
        .clear_state_program
        .as_ref()
        .filter(|value| !value.is_empty())
    {
        val = MsgPack::new();
        val.write_bin(program);
        entries.push(("apsu", std::mem::take(&mut val.buf)));
    }
    if let Some(extra_pages) = tx.extra_program_pages.filter(|value| *value != 0) {
        val = MsgPack::new();
        val.write_u64(u64::from(extra_pages));
        entries.push(("apep", std::mem::take(&mut val.buf)));
    }
    if let Some(grp) = tx.group.filter(|value| *value != [0u8; 32]) {
        val = MsgPack::new();
        val.write_bin(&grp);
        entries.push(("grp", std::mem::take(&mut val.buf)));
    }

    // go-algorand's generated codec writes transaction map keys in bytewise
    // lexicographic order.
    entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));

    m.write_map_header(entries.len());
    for (key, val_bytes) in entries {
        m.write_str(key);
        m.buf.extend_from_slice(&val_bytes);
    }

    m.buf
}

fn encode_bytes_array(values: &[Vec<u8>]) -> Vec<u8> {
    let mut writer = MsgPack::new();
    writer.write_array_header(values.len());
    for value in values {
        writer.write_bin(value);
    }
    writer.buf
}

fn encode_address_array(values: &[Address]) -> Vec<u8> {
    let mut writer = MsgPack::new();
    writer.write_array_header(values.len());
    for value in values {
        writer.write_bin(value.as_bytes());
    }
    writer.buf
}

fn encode_u64_array(values: &[u64]) -> Vec<u8> {
    let mut writer = MsgPack::new();
    writer.write_array_header(values.len());
    for value in values {
        writer.write_u64(*value);
    }
    writer.buf
}

fn encode_box_refs(values: &[BoxReference]) -> Vec<u8> {
    let mut writer = MsgPack::new();
    writer.write_array_header(values.len());
    for value in values {
        let mut entries: Vec<(&'static str, Vec<u8>)> = Vec::new();
        let mut val = MsgPack::new();
        if value.app_index != 0 {
            val.write_u64(value.app_index);
            entries.push(("i", std::mem::take(&mut val.buf)));
        }
        if !value.name.is_empty() {
            val = MsgPack::new();
            val.write_bin(&value.name);
            entries.push(("n", std::mem::take(&mut val.buf)));
        }
        entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
        writer.write_map_header(entries.len());
        for (key, encoded) in entries {
            writer.write_str(key);
            writer.write_raw(&encoded);
        }
    }
    writer.buf
}

fn encode_state_schema(schema: StateSchema) -> Vec<u8> {
    let mut entries: Vec<(&'static str, Vec<u8>)> = Vec::new();
    let mut val = MsgPack::new();

    if schema.num_byte_slice != 0 {
        val.write_u64(schema.num_byte_slice);
        entries.push(("nbs", std::mem::take(&mut val.buf)));
    }
    if schema.num_uint != 0 {
        val = MsgPack::new();
        val.write_u64(schema.num_uint);
        entries.push(("nui", std::mem::take(&mut val.buf)));
    }

    entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    let mut writer = MsgPack::new();
    writer.write_map_header(entries.len());
    for (key, value) in entries {
        writer.write_str(key);
        writer.write_raw(&value);
    }
    writer.buf
}

fn encode_asset_params(params: &AssetCreateParams, include_zero_authorities: bool) -> Vec<u8> {
    let mut entries: Vec<(&'static str, Vec<u8>)> = Vec::new();
    let mut val = MsgPack::new();

    if let Some(metadata_hash) = params.metadata_hash.filter(|value| *value != [0u8; 32]) {
        val.write_bin(&metadata_hash);
        entries.push(("am", std::mem::take(&mut val.buf)));
    }
    if !params.asset_name.is_empty() {
        val = MsgPack::new();
        val.write_str(&params.asset_name);
        entries.push(("an", std::mem::take(&mut val.buf)));
    }
    if !params.url.is_empty() {
        val = MsgPack::new();
        val.write_str(&params.url);
        entries.push(("au", std::mem::take(&mut val.buf)));
    }
    if let Some(clawback) = params
        .clawback
        .filter(|value| include_zero_authorities || *value != Address::zero())
    {
        val = MsgPack::new();
        val.write_bin(clawback.as_bytes());
        entries.push(("c", std::mem::take(&mut val.buf)));
    }
    if params.decimals != 0 {
        val = MsgPack::new();
        val.write_u64(u64::from(params.decimals));
        entries.push(("dc", std::mem::take(&mut val.buf)));
    }
    if params.default_frozen {
        val = MsgPack::new();
        val.write_bool(true);
        entries.push(("df", std::mem::take(&mut val.buf)));
    }
    if let Some(freeze) = params
        .freeze
        .filter(|value| include_zero_authorities || *value != Address::zero())
    {
        val = MsgPack::new();
        val.write_bin(freeze.as_bytes());
        entries.push(("f", std::mem::take(&mut val.buf)));
    }
    if let Some(manager) = params
        .manager
        .filter(|value| include_zero_authorities || *value != Address::zero())
    {
        val = MsgPack::new();
        val.write_bin(manager.as_bytes());
        entries.push(("m", std::mem::take(&mut val.buf)));
    }
    if let Some(reserve) = params
        .reserve
        .filter(|value| include_zero_authorities || *value != Address::zero())
    {
        val = MsgPack::new();
        val.write_bin(reserve.as_bytes());
        entries.push(("r", std::mem::take(&mut val.buf)));
    }
    if params.total != 0 {
        val = MsgPack::new();
        val.write_u64(params.total);
        entries.push(("t", std::mem::take(&mut val.buf)));
    }
    if !params.unit_name.is_empty() {
        val = MsgPack::new();
        val.write_str(&params.unit_name);
        entries.push(("un", std::mem::take(&mut val.buf)));
    }

    entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));

    let mut writer = MsgPack::new();
    writer.write_map_header(entries.len());
    for (key, value) in entries {
        writer.write_str(key);
        writer.write_raw(&value);
    }
    writer.buf
}

/// Encode a signed transaction to canonical Algorand msgpack.
///
/// Output shape: `{ "lsig"?: ..., "sig"?: ..., "txn": {...} }`.
pub fn encode_signed_tx(stx: &SignedTransaction) -> Vec<u8> {
    let mut m = MsgPack::new();

    // Count top-level fields: txn always present; sig or lsig present.
    let mut n = 1usize;
    if stx.sig.is_some() {
        n += 1;
    }
    if stx.lsig.is_some() {
        n += 1;
    }
    m.write_map_header(n);

    // Canonical bytewise lexicographic order: "lsig" < "sig" < "txn".
    if let Some(lsig) = &stx.lsig {
        m.write_str("lsig");
        encode_logicsig(&mut m, lsig);
    }
    if let Some(sig) = &stx.sig {
        m.write_str("sig");
        m.write_bin(&sig.0);
    }
    m.write_str("txn");
    // Embed the txn map bytes directly (already canonical).
    let txn_bytes = encode_transaction(&stx.transaction);
    m.buf.extend_from_slice(&txn_bytes);

    m.buf
}

/// Wrap an unsigned transaction in the SignedTxn container expected by dryrun.
pub fn encode_unsigned_signed_tx(transaction: &TransactionFields) -> Vec<u8> {
    let encoded_transaction = encode_transaction(transaction);
    let mut writer = MsgPack::new();
    writer.write_map_header(1);
    writer.write_str("txn");
    writer.write_raw(&encoded_transaction);
    writer.buf
}

/// Encode an algod dryrun request with synthetic account state.
pub fn encode_dryrun_request(
    signed_transactions: &[Vec<u8>],
    accounts: &[serde_json::Value],
    round: Round,
    protocol_version: &str,
) -> opennodia_core::Result<Vec<u8>> {
    let mut writer = MsgPack::new();
    writer.write_map_header(4);
    writer.write_str("accounts");
    writer.write_array_header(accounts.len());
    for account in accounts {
        writer.write_json(account)?;
    }
    writer.write_str("protocol-version");
    writer.write_str(protocol_version);
    writer.write_str("round");
    writer.write_u64(round.as_u64());
    writer.write_str("txns");
    writer.write_array_header(signed_transactions.len());
    for transaction in signed_transactions {
        writer.write_raw(transaction);
    }
    Ok(writer.buf)
}

/// Encode a LogicSig object: `{ "l": bytes, "ar": [bytes,...], "sig"?: bytes }`.
fn encode_logicsig(m: &mut MsgPack, lsig: &LogicSig) {
    let mut n = 1usize; // "l" is always present
    if !lsig.args.is_empty() {
        n += 1;
    }
    if lsig.sig.is_some() {
        n += 1;
    }
    m.write_map_header(n);
    // Canonical order follows bytewise lexicographic map-key ordering.
    if !lsig.args.is_empty() {
        m.write_str("arg");
        m.write_array_header(lsig.args.len());
        for arg in &lsig.args {
            m.write_bin(arg);
        }
    }
    m.write_str("l");
    m.write_bin(&lsig.logic);
    if let Some(sig) = &lsig.sig {
        m.write_str("sig");
        m.write_bin(&sig.0);
    }
}

// ============================================================================
// Group ID calculation
// ============================================================================

/// Assign a group ID to a list of transactions.
///
/// Group ID = `SHA-512/256("TG" || msgpack({"txlist": [txid, ...]}))`.
///
/// Each transaction ID is `SHA-512/256("TX" || canonical_tx_without_group)`.
/// This matches go-algorand's `transactions.TxGroup` hashing contract.
pub fn assign_group_id(transactions: &mut [TransactionFields]) -> [u8; 32] {
    let txids: Vec<[u8; 32]> = transactions
        .iter()
        .map(transaction_id_without_group)
        .collect();

    let mut group = MsgPack::new();
    group.write_map_header(1);
    group.write_str("txlist");
    group.write_array_header(txids.len());
    for txid in &txids {
        group.write_bin(txid);
    }

    let mut hasher = Sha512_256::new();
    hasher.update(b"TG");
    hasher.update(&group.buf);
    let digest = hasher.finalize();
    let mut gid = [0u8; 32];
    gid.copy_from_slice(&digest[..32]);

    for tx in transactions.iter_mut() {
        tx.group = Some(gid);
    }
    gid
}

/// Compute the canonical Algorand transaction ID digest with Group omitted.
fn transaction_id_without_group(transaction: &TransactionFields) -> [u8; 32] {
    let mut transaction = transaction.clone();
    transaction.group = None;
    let encoded = encode_transaction(&transaction);
    let mut hasher = Sha512_256::new();
    hasher.update(b"TX");
    hasher.update(encoded);
    let digest = hasher.finalize();
    let mut txid = [0u8; 32];
    txid.copy_from_slice(&digest);
    txid
}

/// Sign a transaction with a LogicSig (program-only, no delegation).
///
/// Returns the canonical signed-transaction msgpack bytes.
pub fn sign_with_logicsig(transaction: TransactionFields, program: Vec<u8>) -> Vec<u8> {
    let stx = SignedTransaction::with_logicsig(transaction, program);
    encode_signed_tx(&stx)
}

// ============================================================================
// algod integration helpers
// ============================================================================

#[derive(Debug, Deserialize)]
struct SuggestedParamsResponse {
    #[serde(rename = "last-round")]
    last_round: u64,
    #[serde(rename = "min-fee")]
    min_fee: u64,
    #[serde(rename = "genesis-id")]
    genesis_id: String,
    #[serde(rename = "genesis-hash")]
    genesis_hash: String,
}

fn transaction_params_from_response(
    parsed: SuggestedParamsResponse,
) -> opennodia_core::Result<TransactionParams> {
    let gh_bytes = base64::engine::general_purpose::STANDARD
        .decode(&parsed.genesis_hash)
        .map_err(|e| opennodia_core::Error::Algod(format!("genesis hash b64: {e}")))?;
    let genesis_hash: [u8; 32] = gh_bytes.try_into().map_err(|bytes: Vec<u8>| {
        opennodia_core::Error::Algod(format!(
            "genesis hash must be 32 bytes, got {}",
            bytes.len()
        ))
    })?;

    Ok(TransactionParams {
        fee: parsed.min_fee,
        first_valid: Round(parsed.last_round),
        last_valid: Round(parsed.last_round.saturating_add(1000)),
        genesis_id: parsed.genesis_id,
        genesis_hash,
    })
}

/// Fetch suggested transaction parameters from algod (`/v2/transactions/params`).
pub async fn fetch_tx_params(
    algod: &opennodia_node::AlgodClient,
) -> opennodia_core::Result<TransactionParams> {
    let url = format!("{}/v2/transactions/params", algod.base_url());
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("X-Algo-API-Token", algod.token())
        .send()
        .await
        .map_err(|e| opennodia_core::Error::Algod(format!("tx params: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(opennodia_core::Error::Algod(format!(
            "tx params {status}: {body}"
        )));
    }
    let parsed: SuggestedParamsResponse = resp
        .json()
        .await
        .map_err(|e| opennodia_core::Error::Algod(format!("tx params decode: {e}")))?;

    transaction_params_from_response(parsed)
}

/// Submit a signed transaction group to algod and return the txid.
pub async fn submit_signed_tx(
    algod: &opennodia_node::AlgodClient,
    signed_group_bytes: &[u8],
) -> opennodia_core::Result<String> {
    let url = format!("{}/v2/transactions", algod.base_url());
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("X-Algo-API-Token", algod.token())
        .header("Content-Type", "application/x-binary")
        .body(signed_group_bytes.to_vec())
        .send()
        .await
        .map_err(|e| opennodia_core::Error::Algod(format!("submit: {e}")))?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(opennodia_core::Error::Algod(format!(
            "submit {status}: {body}"
        )));
    }
    // The transaction was accepted (2xx). Extract the txid from the response.
    // algod returns {"txId": "..."} (capital I) as JSON, but some relay/proxy
    // setups may alter the casing, Content-Type, or body. We try JSON first
    // (accepting both casings), then fall back to a raw search so we never
    // report a false failure for an accepted tx.
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct SubmitResp {
        tx_id: String,
    }
    if let Ok(parsed) = serde_json::from_str::<SubmitResp>(&body) {
        return Ok(parsed.tx_id);
    }
    // Fallback: search for "txId"/"txid" field in the raw body.
    if let Some(txid) = extract_txid_from_text(&body) {
        tracing::warn!(
            body = %body,
            "submit response was not valid JSON but txid extracted"
        );
        return Ok(txid);
    }
    // The tx was accepted (2xx) but we cannot read the txid. This is not a
    // submission failure — warn and return an error that makes clear the tx
    // may already be on-chain.
    tracing::error!(status = %status, body = %body, "accepted tx but unreadable txid");
    Err(opennodia_core::Error::Algod(format!(
        "transaction accepted by algod (HTTP {status}) but txid could not be read from response; the transaction may already be confirmed — check the account on a block explorer"
    )))
}

/// Best-effort extraction of a txid from a response body that may not be
/// strict JSON (e.g. wrapped by a proxy or with unexpected whitespace).
fn extract_txid_from_text(body: &str) -> Option<String> {
    // algod uses "txId" (capital I); accept both casings.
    let lower = body.to_ascii_lowercase();
    let key = "\"txid\"";
    let start = lower.find(key)?;
    let rest = &body[start + key.len()..];
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(':')?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Wait for a transaction to be confirmed by polling algod.
pub async fn wait_for_confirmation(
    algod: &opennodia_node::AlgodClient,
    txid: &str,
    timeout_rounds: u64,
) -> opennodia_core::Result<u64> {
    let start = algod.status().await?.last_round.0;
    let deadline = start + timeout_rounds;
    loop {
        let current = algod.status().await?.last_round.0;
        if current > deadline {
            return Err(opennodia_core::Error::Algod(format!(
                "confirmation timeout for {txid}"
            )));
        }
        let url = format!("{}/v2/transactions/pending/{txid}", algod.base_url());
        let client = reqwest::Client::new();
        let resp = client
            .get(&url)
            .header("X-Algo-API-Token", algod.token())
            .send()
            .await
            .map_err(|e| opennodia_core::Error::Algod(format!("pending: {e}")))?;
        if resp.status().is_success() {
            #[derive(serde::Deserialize)]
            struct PendingResp {
                #[serde(rename = "confirmed-round", default)]
                confirmed_round: u64,
                #[serde(rename = "pool-error", default)]
                pool_error: String,
            }
            let parsed: PendingResp = resp
                .json()
                .await
                .map_err(|e| opennodia_core::Error::Algod(format!("pending decode: {e}")))?;
            if parsed.confirmed_round > 0 {
                return Ok(parsed.confirmed_round);
            }
            if !parsed.pool_error.is_empty() {
                return Err(opennodia_core::Error::Algod(format!(
                    "pool error: {pool_error}",
                    pool_error = parsed.pool_error
                )));
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

/// A human-readable transaction preview for display before signing.
#[derive(Clone, Debug, serde::Serialize)]
pub struct TxPreview {
    pub summary: String,
    pub ty: String,
    pub sender_label: String,
    pub receiver_label: String,
    pub amount_label: String,
    pub fee_label: String,
}

/// Build a human-readable preview of a transaction.
pub fn preview_transaction(tx: &TransactionFields) -> TxPreview {
    let summary = match tx.ty {
        TransactionType::Pay => {
            let amt = tx.amount.unwrap_or(0);
            format!(
                "Send {} microAlgo to {}",
                amt,
                tx.receiver
                    .map(|a| format!("{a}"))
                    .unwrap_or_else(|| "?".into())
            )
        }
        TransactionType::Axfer => {
            let amt = tx.asset_amount.unwrap_or(0);
            format!(
                "Transfer {} units of asset {} to {}",
                amt,
                tx.xfer_asset.unwrap_or(0),
                tx.asset_receiver
                    .map(|a| format!("{a}"))
                    .unwrap_or_else(|| "?".into())
            )
        }
        TransactionType::Acfg => {
            let params = tx.asset_params.as_ref();
            format!(
                "Create ASA {} ({}) with total {}",
                params
                    .map(|params| params.asset_name.as_str())
                    .filter(|value| !value.is_empty())
                    .unwrap_or("unnamed"),
                params
                    .map(|params| params.unit_name.as_str())
                    .filter(|value| !value.is_empty())
                    .unwrap_or("no unit"),
                params.map(|params| params.total).unwrap_or(0)
            )
        }
        TransactionType::Appl => {
            if tx.app_id.unwrap_or(0) == 0 {
                "Create application".to_string()
            } else {
                format!("Call application {}", tx.app_id.unwrap_or(0))
            }
        }
        other => format!("{other:?} transaction"),
    };
    TxPreview {
        summary,
        ty: tx.ty.as_str().to_string(),
        sender_label: format!("{}", tx.sender),
        receiver_label: tx
            .receiver
            .or(tx.asset_receiver)
            .map(|a| format!("{a}"))
            .unwrap_or_else(|| "—".into()),
        amount_label: format!("{}", tx.amount.or(tx.asset_amount).unwrap_or(0)),
        fee_label: format!("{} microAlgo", tx.fee),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_params() -> TransactionParams {
        TransactionParams::new(Round(1000), "testnet-v1.0".into(), [0xaa; 32])
    }

    #[test]
    fn suggested_params_use_algod_response_fields() {
        let genesis_hash = base64::engine::general_purpose::STANDARD.encode([0xaa; 32]);
        let parsed: SuggestedParamsResponse = serde_json::from_value(serde_json::json!({
            "fee": 0,
            "last-round": 42,
            "min-fee": 1000,
            "genesis-id": "testnet-v1.0",
            "genesis-hash": genesis_hash
        }))
        .unwrap();

        let params = transaction_params_from_response(parsed).unwrap();
        assert_eq!(params.fee, 1000);
        assert_eq!(params.first_valid, Round(42));
        assert_eq!(params.last_valid, Round(1042));
        assert_eq!(params.genesis_id, "testnet-v1.0");
        assert_eq!(params.genesis_hash, [0xaa; 32]);
    }

    #[test]
    fn encode_payment_has_canonical_fields() {
        let p = sample_params();
        let tx = build_payment(
            Address::from_bytes([1u8; 32]),
            Address::from_bytes([2u8; 32]),
            1_000_000,
            &p,
        );
        let bytes = encode_transaction(&tx);
        assert!(!bytes.is_empty());
        // Map header should be present (fixmap 0x80..0x8f or map16/map32).
        let first = bytes[0];
        assert!(
            (0x80..=0x8f).contains(&first) || first == 0xde || first == 0xdf,
            "expected msgpack map header, got {first:#x}"
        );
    }

    #[test]
    fn encode_roundtrip_field_preservation() {
        // We don't decode yet, but we can re-encode and confirm determinism.
        let p = sample_params();
        let tx = build_asset_transfer(
            Address::from_bytes([3u8; 32]),
            Address::from_bytes([4u8; 32]),
            12345,
            500,
            &p,
        );
        let b1 = encode_transaction(&tx);
        let b2 = encode_transaction(&tx);
        assert_eq!(b1, b2, "encoding must be deterministic");
    }

    #[test]
    fn group_id_is_consistent_and_shared() {
        let p = sample_params();
        let mut txs = vec![
            build_payment(Address::zero(), Address::zero(), 100, &p),
            build_payment(Address::zero(), Address::zero(), 200, &p),
        ];
        let gid = assign_group_id(&mut txs);
        assert_eq!(txs[0].group, Some(gid));
        assert_eq!(txs[1].group, Some(gid));
        // Re-assigning with the same txs (after clearing) yields the same gid.
        for tx in txs.iter_mut() {
            tx.group = None;
        }
        let gid2 = assign_group_id(&mut txs);
        assert_eq!(gid, gid2);
    }

    #[test]
    fn group_id_changes_when_txs_change() {
        let p = sample_params();
        let mut g1 = vec![build_payment(Address::zero(), Address::zero(), 100, &p)];
        let mut g2 = vec![build_payment(Address::zero(), Address::zero(), 999, &p)];
        let id1 = assign_group_id(&mut g1);
        let id2 = assign_group_id(&mut g2);
        assert_ne!(id1, id2);
    }

    #[test]
    fn box_reference_omits_zero_app_index() {
        let encoded = encode_box_refs(&[BoxReference {
            app_index: 0,
            name: vec![1, 2, 3],
        }]);

        assert!(encoded.windows(2).any(|window| window == b"\xa1n"));
        assert!(!encoded.windows(2).any(|window| window == b"\xa1i"));
    }

    #[test]
    fn logicsig_signed_tx_includes_lsig_and_txn() {
        let p = sample_params();
        let tx = build_payment(Address::zero(), Address::zero(), 1, &p);
        let stx = SignedTransaction::with_logicsig(tx, vec![0x01, 0x02]);
        let bytes = encode_signed_tx(&stx);
        // Top-level map header.
        assert!((0x80..=0x8f).contains(&bytes[0]) || bytes[0] == 0xde);
        // Should contain the "lsig" and "txn" key bytes somewhere.
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("lsig") || bytes.windows(4).any(|w| w == b"lsig"));
        assert!(s.contains("txn") || bytes.windows(3).any(|w| w == b"txn"));
    }

    #[test]
    fn opt_in_is_zero_amount_self_transfer() {
        let p = sample_params();
        let acct = Address::from_bytes([5u8; 32]);
        let tx = build_asset_opt_in(acct, 999, &p);
        assert_eq!(tx.ty, TransactionType::Axfer);
        assert_eq!(tx.asset_amount, Some(0));
        assert_eq!(tx.asset_receiver, Some(acct));
        assert_eq!(tx.sender, acct);
        assert_eq!(tx.xfer_asset, Some(999));
    }

    #[test]
    fn asset_create_encodes_asset_params() {
        let p = sample_params();
        let creator = Address::from_bytes([6u8; 32]);
        let manager = Address::from_bytes([7u8; 32]);
        let tx = build_asset_create(
            creator,
            AssetCreateParams {
                total: 1_000_000,
                decimals: 6,
                default_frozen: false,
                unit_name: "QAT".into(),
                asset_name: "QA Token".into(),
                url: "https://example.invalid/asset.json".into(),
                metadata_hash: Some([8u8; 32]),
                manager: Some(manager),
                reserve: None,
                freeze: None,
                clawback: None,
            },
            &p,
        );

        assert_eq!(tx.ty, TransactionType::Acfg);
        assert_eq!(tx.sender, creator);
        assert_eq!(tx.asset_params.as_ref().unwrap().total, 1_000_000);

        let bytes = encode_transaction(&tx);
        assert!(bytes.windows(4).any(|window| window == b"apar"));
        assert!(bytes.windows(2).any(|window| window == b"an"));
        assert!(bytes.windows(2).any(|window| window == b"dc"));
        assert!(bytes.windows(2).any(|window| window == b"un"));
    }

    #[test]
    fn application_create_encodes_programs_and_schema() {
        let p = sample_params();
        let tx = build_application_create(
            Address::from_bytes([8u8; 32]),
            vec![1, 32, 1, 1, 34],
            vec![1, 32, 1, 1, 34],
            StateSchema::new(8, 2),
            StateSchema::new(0, 0),
            vec![b"create".to_vec()],
            &p,
        );

        assert_eq!(tx.ty, TransactionType::Appl);
        assert_eq!(tx.app_id, None);
        assert_eq!(tx.on_completion, Some(OnCompletion::NoOp));

        let bytes = encode_transaction(&tx);
        assert!(bytes.windows(4).any(|window| window == b"apap"));
        assert!(bytes.windows(4).any(|window| window == b"apsu"));
        assert!(bytes.windows(4).any(|window| window == b"apgs"));
        assert!(bytes.windows(3).any(|window| window == b"nui"));
        assert!(bytes.windows(3).any(|window| window == b"nbs"));
        assert!(bytes.windows(4).any(|window| window == b"apaa"));
    }

    #[test]
    fn application_call_encodes_references() {
        let p = sample_params();
        let account = Address::from_bytes([9u8; 32]);
        let tx = build_application_call(
            Address::from_bytes([8u8; 32]),
            1234,
            vec![b"swap".to_vec(), 55u64.to_be_bytes().to_vec()],
            vec![account],
            vec![42, 77],
            vec![999],
            &p,
        );

        assert_eq!(tx.ty, TransactionType::Appl);
        assert_eq!(tx.app_id, Some(1234));

        let bytes = encode_transaction(&tx);
        assert!(bytes.windows(4).any(|window| window == b"apid"));
        assert!(bytes.windows(4).any(|window| window == b"apaa"));
        assert!(bytes.windows(4).any(|window| window == b"apat"));
        assert!(bytes.windows(4).any(|window| window == b"apas"));
        assert!(bytes.windows(4).any(|window| window == b"apfa"));
    }

    #[test]
    fn application_opt_in_encodes_on_completion() {
        let p = sample_params();
        let tx = build_application_opt_in(
            Address::from_bytes([8u8; 32]),
            1234,
            vec![b"lp".to_vec()],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            &p,
        );

        assert_eq!(tx.ty, TransactionType::Appl);
        assert_eq!(tx.app_id, Some(1234));
        assert_eq!(tx.on_completion, Some(OnCompletion::OptIn));

        let bytes = encode_transaction(&tx);
        assert!(bytes.windows(4).any(|window| window == b"apid"));
        assert!(bytes.windows(4).any(|window| window == b"apan"));
    }

    #[test]
    fn preview_payment() {
        let p = sample_params();
        let tx = build_payment(Address::zero(), Address::zero(), 1_000_000, &p);
        let pv = preview_transaction(&tx);
        assert!(pv.summary.contains("microAlgo"));
        assert_eq!(pv.ty, "pay");
    }

    #[test]
    fn extract_txid_from_valid_json() {
        let body = r#"{"txid":"ABC123XYZ"}"#;
        assert_eq!(extract_txid_from_text(body).as_deref(), Some("ABC123XYZ"));
    }

    #[test]
    fn extract_txid_from_json_with_whitespace() {
        // A proxy might reformat the JSON.
        let body = r#"{"txid" : "DEF456"}"#;
        assert_eq!(extract_txid_from_text(body).as_deref(), Some("DEF456"));
    }

    #[test]
    fn extract_txid_from_malformed_body_returns_none() {
        assert_eq!(extract_txid_from_text("not json"), None);
        assert_eq!(extract_txid_from_text(r#"{"error":"nope"}"#), None);
    }
}
