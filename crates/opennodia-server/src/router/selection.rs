use super::dto::UnifiedRouteQuote;

pub(super) fn amount_out_after_network_fee(candidate: &UnifiedRouteQuote) -> u64 {
    if candidate.asset_out == 0 {
        candidate
            .amount_out
            .saturating_sub(candidate.network_fee_microalgo)
    } else {
        candidate.amount_out
    }
}

pub(super) fn compare_candidates(
    left: &UnifiedRouteQuote,
    right: &UnifiedRouteQuote,
) -> std::cmp::Ordering {
    let left_final = amount_out_after_network_fee(left);
    let right_final = amount_out_after_network_fee(right);
    right_final
        .cmp(&left_final)
        .then_with(|| right.amount_out.cmp(&left.amount_out))
        .then_with(|| left.network_fee_microalgo.cmp(&right.network_fee_microalgo))
        .then_with(|| left.source_id.cmp(&right.source_id))
}

pub(super) fn source_matches(filter: &str, candidate: &UnifiedRouteQuote) -> bool {
    let filter = filter.trim();
    filter.is_empty()
        || filter == "best"
        || filter == candidate.source_type
        || filter == candidate.source_id
        || filter == candidate.canonical_id
        || (filter == "native_split" && candidate.source_id == "native_split")
        || (filter == "tinyman" && candidate.source_id == "external_tinyman")
        || (filter == "pact" && candidate.source_id == "external_pact")
        || (filter == "native_amm" && candidate.source_type == "native_pool")
}

pub(super) fn exclusion_reason(filter: &str, candidate: &UnifiedRouteQuote) -> Option<String> {
    if !source_matches(filter, candidate) {
        return Some(format!("excluded by source filter '{filter}'"));
    }
    if !candidate.executable {
        return Some(
            "excluded because the source is quote-only or unsupported for swap submit".into(),
        );
    }
    if candidate.remaining_input != 0 {
        return Some(format!(
            "excluded because {} units of input asset {} would remain unfilled",
            candidate.remaining_input, candidate.asset_in
        ));
    }
    None
}
