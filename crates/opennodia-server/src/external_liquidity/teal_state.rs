use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum StateValue {
    Uint(u64),
    Bytes(Vec<u8>),
}

pub(super) fn teal_state_map(entries: &[TealKeyValue]) -> ApiResult<HashMap<String, StateValue>> {
    let mut out = HashMap::new();
    for entry in entries {
        let key = base64::engine::general_purpose::STANDARD
            .decode(&entry.key)
            .map_err(|error| internal(format!("decode TEAL state key: {error}")))?;
        let key = String::from_utf8(key)
            .map_err(|error| internal(format!("TEAL state key is not UTF-8: {error}")))?;
        out.insert(key, teal_value(entry.value.clone())?);
    }
    Ok(out)
}

fn teal_value(value: TealValue) -> ApiResult<StateValue> {
    match value.value_type {
        1 => Ok(StateValue::Bytes(
            base64::engine::general_purpose::STANDARD
                .decode(value.bytes)
                .map_err(|error| internal(format!("decode TEAL bytes value: {error}")))?,
        )),
        2 => Ok(StateValue::Uint(value.uint)),
        other => Err(internal(format!("unsupported TEAL value type: {other}"))),
    }
}

pub(super) fn state_uint(state: &HashMap<String, StateValue>, key: &str) -> ApiResult<u64> {
    match state.get(key) {
        Some(StateValue::Uint(value)) => Ok(*value),
        Some(StateValue::Bytes(_)) => Err(bad_request(format!("state key {key} must be uint"))),
        None => Err(bad_request(format!("missing state key: {key}"))),
    }
}

pub(super) fn optional_state_uint(state: &HashMap<String, StateValue>, key: &str) -> Option<u64> {
    match state.get(key) {
        Some(StateValue::Uint(value)) => Some(*value),
        _ => None,
    }
}

pub(super) fn state_bytes(state: &HashMap<String, StateValue>, key: &str) -> ApiResult<Vec<u8>> {
    match state.get(key) {
        Some(StateValue::Bytes(value)) => Ok(value.clone()),
        Some(StateValue::Uint(_)) => Err(bad_request(format!("state key {key} must be bytes"))),
        None => Err(bad_request(format!("missing state key: {key}"))),
    }
}

pub(super) fn optional_state_bytes_raw_key(
    state: &HashMap<String, StateValue>,
    raw_key: &[u8],
) -> Option<Vec<u8>> {
    let key = String::from_utf8(raw_key.to_vec()).ok()?;
    match state.get(&key) {
        Some(StateValue::Bytes(value)) => Some(value.clone()),
        _ => None,
    }
}

pub(super) fn optional_state_text(
    state: &HashMap<String, StateValue>,
    key: &str,
) -> ApiResult<Option<String>> {
    match state.get(key) {
        Some(StateValue::Bytes(value)) => {
            Ok(Some(String::from_utf8(value.clone()).map_err(|error| {
                bad_request(format!("state key {key} is not UTF-8: {error}"))
            })?))
        }
        Some(StateValue::Uint(_)) => Err(bad_request(format!("state key {key} must be bytes"))),
        None => Ok(None),
    }
}

pub(super) fn decode_u64_list(bytes: &[u8]) -> ApiResult<Vec<u64>> {
    if !bytes.len().is_multiple_of(8) {
        return Err(bad_request(format!(
            "uint64 byte list length must be a multiple of 8, got {}",
            bytes.len()
        )));
    }
    Ok(bytes
        .chunks_exact(8)
        .map(|chunk| {
            u64::from_be_bytes(
                chunk
                    .try_into()
                    .expect("chunks_exact(8) always yields 8 bytes"),
            )
        })
        .collect())
}

pub(super) fn uint_from_be_bytes(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .fold(0u64, |value, byte| (value << 8) | u64::from(*byte))
}
