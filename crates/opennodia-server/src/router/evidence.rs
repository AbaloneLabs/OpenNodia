use sha2::{Digest, Sha256};

use super::dto::{RouterQuoteRequest, UnifiedRouteQuote};

pub(super) fn quote_id(
    network: &str,
    req: &RouterQuoteRequest,
    source_filter: &str,
    selected_route_hash: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"opennodia.router.quote.v1");
    hash_field(&mut hasher, network);
    hash_field(&mut hasher, &req.asset_in.to_string());
    hash_field(&mut hasher, &req.asset_out.to_string());
    hash_field(&mut hasher, &req.amount_in.to_string());
    hash_field(&mut hasher, &req.slippage_bps.to_string());
    hash_field(&mut hasher, source_filter);
    hash_field(&mut hasher, selected_route_hash);
    hex::encode(hasher.finalize())
}

pub(super) fn route_hash(network: &str, quote: &UnifiedRouteQuote) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"opennodia.router.route.v1");
    hash_field(&mut hasher, network);
    hash_field(&mut hasher, &quote.source_type);
    hash_field(&mut hasher, &quote.source_id);
    hash_field(&mut hasher, &quote.execution);
    hash_field(&mut hasher, &quote.canonical_id);
    hash_field(&mut hasher, &quote.asset_in.to_string());
    hash_field(&mut hasher, &quote.asset_out.to_string());
    hash_field(&mut hasher, &quote.amount_in.to_string());
    hash_field(&mut hasher, &quote.input_consumed.to_string());
    hash_field(&mut hasher, &quote.remaining_input.to_string());
    hash_field(&mut hasher, &quote.amount_out.to_string());
    hash_field(&mut hasher, &quote.minimum_out.to_string());
    hash_field(&mut hasher, &quote.lp_fee_bps.to_string());
    hash_field(&mut hasher, &quote.network_fee_microalgo.to_string());
    hash_field(&mut hasher, &quote.split_legs.len().to_string());
    for leg in &quote.split_legs {
        hash_field(&mut hasher, &leg.source_type);
        hash_field(&mut hasher, &leg.source_id);
        hash_field(&mut hasher, &leg.canonical_id);
        hash_field(&mut hasher, &leg.app_id.unwrap_or_default().to_string());
        hash_field(&mut hasher, leg.pool_id.as_deref().unwrap_or(""));
        hash_field(&mut hasher, &leg.amount_in.to_string());
        hash_field(&mut hasher, &leg.amount_out.to_string());
        hash_field(&mut hasher, &leg.minimum_out.to_string());
        hash_field(&mut hasher, &leg.network_fee_microalgo.to_string());
    }
    hex::encode(hasher.finalize())
}

fn hash_field(hasher: &mut Sha256, value: &str) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value.as_bytes());
}
