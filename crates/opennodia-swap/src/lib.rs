//! Atomic swap transaction builder for OpenNodia DEX.
//!
//! This crate implements the non-custodial DEX primitives:
//! - [`escrow`]: security-hardened TEAL LogicSig escrow programs (Phase 1).
//! - [`tx`]: Algorand transaction construction, msgpack encoding, group ID
//!   calculation, and signing (Phase 2).
//! - [`order`]: order types and lifecycle status.
//! - [`create`], [`verify`], [`fill`], [`cancel`], [`link`]: order lifecycle
//!   (Phase 3).
//! - [`matching`]: smart routing and matching engine (Phase 4).

pub mod cancel;
pub mod create;
pub mod escrow;
pub mod fill;
pub mod link;
pub mod matching;
pub mod order;
pub mod tx;
pub mod verify;

pub use cancel::{build_cancel_group, CancelResult};
pub use create::{build_deposit_group, split_amounts, CreateOrderResult};
pub use escrow::{
    cancel_note, escrow_address, render_program, validate_params, EscrowAccount, EscrowKind,
    EscrowParams, BASE_ESCROW_FUNDING_MICROALGO, CANCEL_NOTE_PREFIX, DEFAULT_MAX_FEE,
    MAX_LOGICSIG_PROGRAM_BYTES, MIN_ESCROW_FUNDING_MICROALGO,
};
pub use fill::{build_fill_group, derive_lease, fill_allowed, FillResult};
pub use link::{decode_order_link, encode_order_link, OrderLinkPayload};
pub use matching::{
    compute_fill_stats, estimate_batch_count, match_order, route_order, BookOrder, FillCandidate,
    FillStats, InMemoryOrderbook, MatchResult, OrderRequest, OrderbookSource, RoutingDecision,
};
pub use order::{Order, OrderSide, OrderStatus};
pub use tx::{
    assign_group_id, build_application, build_application_call, build_application_create,
    build_application_opt_in, build_asset_config, build_asset_create, build_asset_opt_in,
    build_asset_transfer, build_payment, encode_dryrun_request, encode_signed_tx,
    encode_transaction, encode_unsigned_signed_tx, fetch_tx_params, preview_transaction,
    sign_with_logicsig, submit_signed_tx, wait_for_confirmation, ApplicationCallFields,
    AssetCreateParams, BoxReference, LogicSig, OnCompletion, SignedTransaction, StateSchema,
    TransactionFields, TransactionParams, TransactionType, TxPreview,
};
pub use verify::{verify_escrow, OrderVerification};
