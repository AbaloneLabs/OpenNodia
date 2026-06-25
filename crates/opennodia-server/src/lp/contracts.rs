use base64::Engine;
use opennodia_amm::{
    PoolGlobalState, PoolGlobalValue, GLOBAL_KEY_POOL_APPROVAL_HASH, GLOBAL_KEY_POOL_CLEAR_HASH,
    GLOBAL_KEY_REGISTRY_ACTIVE_COUNT, GLOBAL_KEY_REGISTRY_GENESIS_HASH,
    GLOBAL_KEY_REGISTRY_VERSION,
};
use opennodia_node::{AlgodClient, ApplicationInfo, TealKeyValue, TealValue};
use sha2::{Digest, Sha512_256};
use std::collections::HashMap;

use super::{bad_request, internal, service_unavailable, ApiResult};

#[derive(Debug, Clone)]
pub(super) struct NativePoolPrograms {
    pub(super) approval_programs: Vec<Vec<u8>>,
    pub(super) clear_state_program: Vec<u8>,
}

#[derive(Debug, Clone)]
pub(super) struct NativeRegistryPrograms {
    pub(super) approval_program: Vec<u8>,
    pub(super) clear_state_program: Vec<u8>,
}

pub(super) async fn compile_native_pool_programs(
    algod: &AlgodClient,
) -> ApiResult<NativePoolPrograms> {
    let mut approval_programs = Vec::new();
    for version in [
        opennodia_amm::CONTRACT_VERSION_V2,
        opennodia_amm::CONTRACT_VERSION_V3,
    ] {
        let approval = algod
            .compile_teal(
                opennodia_amm::contract::approval_program_source_for_version(version).as_bytes(),
            )
            .await
            .map_err(|error| {
                service_unavailable(format!("compile AMM v{version} approval program: {error}"))
            })?;
        approval_programs.push(
            base64::engine::general_purpose::STANDARD
                .decode(approval.result)
                .map_err(|error| {
                    internal(format!(
                        "decode compiled v{version} approval program: {error}"
                    ))
                })?,
        );
    }
    let clear = algod
        .compile_teal(opennodia_amm::contract::clear_state_source().as_bytes())
        .await
        .map_err(|error| service_unavailable(format!("compile AMM clear program: {error}")))?;
    let clear_program = base64::engine::general_purpose::STANDARD
        .decode(clear.result)
        .map_err(|error| internal(format!("decode compiled clear program: {error}")))?;
    Ok(NativePoolPrograms {
        approval_programs,
        clear_state_program: clear_program,
    })
}

pub(super) async fn compile_native_registry_programs(
    algod: &AlgodClient,
) -> ApiResult<NativeRegistryPrograms> {
    let approval = algod
        .compile_teal(opennodia_amm::contract::registry_approval_program_source().as_bytes())
        .await
        .map_err(|error| service_unavailable(format!("compile AMM registry program: {error}")))?;
    let clear = algod
        .compile_teal(opennodia_amm::contract::clear_state_source().as_bytes())
        .await
        .map_err(|error| {
            service_unavailable(format!("compile AMM registry clear program: {error}"))
        })?;
    let approval_program = base64::engine::general_purpose::STANDARD
        .decode(approval.result)
        .map_err(|error| internal(format!("decode compiled registry program: {error}")))?;
    let clear_state_program = base64::engine::general_purpose::STANDARD
        .decode(clear.result)
        .map_err(|error| internal(format!("decode compiled registry clear program: {error}")))?;
    Ok(NativeRegistryPrograms {
        approval_program,
        clear_state_program,
    })
}

pub(super) fn current_pool_approval_program(programs: &NativePoolPrograms) -> ApiResult<&Vec<u8>> {
    programs
        .approval_programs
        .last()
        .ok_or_else(|| internal("no compiled native pool approval programs"))
}

fn decode_b64_32(value: &str, field: &str) -> ApiResult<[u8; 32]> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|error| internal(format!("decode {field}: {error}")))?;
    decoded.try_into().map_err(|bytes: Vec<u8>| {
        internal(format!("{field} must be 32 bytes, got {}", bytes.len()))
    })
}

pub(super) async fn genesis_hash(algod: &AlgodClient) -> ApiResult<[u8; 32]> {
    let versions = algod
        .versions()
        .await
        .map_err(|error| service_unavailable(format!("fetch algod versions: {error}")))?;
    decode_b64_32(&versions.genesis_hash_b64, "genesis_hash_b64")
}

fn teal_value_to_pool_value(value: &TealValue, key_name: &str) -> ApiResult<PoolGlobalValue> {
    match value.value_type {
        1 => {
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(&value.bytes)
                .map_err(|error| {
                    bad_request(format!("decode app state bytes {key_name}: {error}"))
                })?;
            Ok(PoolGlobalValue::Bytes(decoded))
        }
        2 => Ok(PoolGlobalValue::Uint(value.uint)),
        other => Err(bad_request(format!(
            "unsupported TEAL state value type {other} for key {key_name}"
        ))),
    }
}

pub(super) fn pool_global_state(entries: &[TealKeyValue]) -> ApiResult<PoolGlobalState> {
    let mut out = HashMap::new();
    for entry in entries {
        let key = base64::engine::general_purpose::STANDARD
            .decode(&entry.key)
            .map_err(|error| bad_request(format!("decode app state key: {error}")))?;
        let key_name = String::from_utf8_lossy(&key);
        let value = teal_value_to_pool_value(&entry.value, &key_name)?;
        out.insert(key, value);
    }
    Ok(out)
}

fn decode_application_program(value: &str, app_id: u64, field: &str) -> ApiResult<Vec<u8>> {
    if value.trim().is_empty() {
        return Err(bad_request(format!(
            "application {app_id} is missing {field}"
        )));
    }
    base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|error| bad_request(format!("decode application {app_id} {field}: {error}")))
}

pub(super) fn verify_native_pool_programs(
    app: &ApplicationInfo,
    expected: &NativePoolPrograms,
) -> ApiResult<()> {
    let approval =
        decode_application_program(&app.params.approval_program, app.id, "approval-program")?;
    if !expected.approval_programs.contains(&approval) {
        return Err(bad_request(format!(
            "application {} is not an OpenNodia native pool: approval program mismatch",
            app.id
        )));
    }

    let clear = decode_application_program(
        &app.params.clear_state_program,
        app.id,
        "clear-state-program",
    )?;
    if clear != expected.clear_state_program {
        return Err(bad_request(format!(
            "application {} is not an OpenNodia native pool: clear-state program mismatch",
            app.id
        )));
    }

    Ok(())
}

pub(super) fn verify_native_registry_programs(
    app: &ApplicationInfo,
    expected: &NativeRegistryPrograms,
) -> ApiResult<()> {
    let approval =
        decode_application_program(&app.params.approval_program, app.id, "approval-program")?;
    if approval != expected.approval_program {
        return Err(bad_request(format!(
            "application {} is not an OpenNodia native registry: approval program mismatch",
            app.id
        )));
    }

    let clear = decode_application_program(
        &app.params.clear_state_program,
        app.id,
        "clear-state-program",
    )?;
    if clear != expected.clear_state_program {
        return Err(bad_request(format!(
            "application {} is not an OpenNodia native registry: clear-state program mismatch",
            app.id
        )));
    }

    Ok(())
}

pub(super) fn program_hash(program: &[u8]) -> [u8; 32] {
    let digest = Sha512_256::digest(program);
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

fn required_global_bytes(
    state: &PoolGlobalState,
    key: &'static [u8],
    name: &'static str,
) -> ApiResult<Vec<u8>> {
    match state.get(key) {
        Some(PoolGlobalValue::Bytes(value)) => Ok(value.clone()),
        Some(PoolGlobalValue::Uint(_)) => Err(bad_request(format!(
            "registry global key {name} has wrong type: expected bytes"
        ))),
        None => Err(bad_request(format!(
            "registry global key {name} is missing"
        ))),
    }
}

fn required_global_uint(
    state: &PoolGlobalState,
    key: &'static [u8],
    name: &'static str,
) -> ApiResult<u64> {
    match state.get(key) {
        Some(PoolGlobalValue::Uint(value)) => Ok(*value),
        Some(PoolGlobalValue::Bytes(_)) => Err(bad_request(format!(
            "registry global key {name} has wrong type: expected uint"
        ))),
        None => Err(bad_request(format!(
            "registry global key {name} is missing"
        ))),
    }
}

pub(super) fn verify_native_registry_state(
    app: &ApplicationInfo,
    expected_pool_programs: &NativePoolPrograms,
    expected_genesis_hash: [u8; 32],
) -> ApiResult<u64> {
    let state = pool_global_state(&app.params.global_state)?;
    let version = required_global_uint(&state, GLOBAL_KEY_REGISTRY_VERSION, "registry_version")?;
    if version != 1 {
        return Err(bad_request(format!(
            "application {} has unsupported native registry version {version}",
            app.id
        )));
    }

    let approval_hash =
        required_global_bytes(&state, GLOBAL_KEY_POOL_APPROVAL_HASH, "pool_approval_hash")?;
    let expected_approval_hash =
        program_hash(current_pool_approval_program(expected_pool_programs)?);
    if approval_hash.as_slice() != expected_approval_hash {
        return Err(bad_request(format!(
            "application {} native registry pool approval hash mismatch",
            app.id
        )));
    }

    let clear_hash = required_global_bytes(&state, GLOBAL_KEY_POOL_CLEAR_HASH, "pool_clear_hash")?;
    let expected_clear_hash = program_hash(&expected_pool_programs.clear_state_program);
    if clear_hash.as_slice() != expected_clear_hash {
        return Err(bad_request(format!(
            "application {} native registry pool clear-state hash mismatch",
            app.id
        )));
    }

    let genesis_hash =
        required_global_bytes(&state, GLOBAL_KEY_REGISTRY_GENESIS_HASH, "genesis_hash")?;
    if genesis_hash.as_slice() != expected_genesis_hash {
        return Err(bad_request(format!(
            "application {} native registry genesis hash mismatch",
            app.id
        )));
    }

    required_global_uint(&state, GLOBAL_KEY_REGISTRY_ACTIVE_COUNT, "active_count")
}

pub(super) async fn validate_native_registry_app(
    algod: &AlgodClient,
    registry_app_id: u64,
    registry_programs: &NativeRegistryPrograms,
    pool_programs: &NativePoolPrograms,
) -> ApiResult<u64> {
    if registry_app_id == 0 {
        return Err(bad_request(
            "native AMM registry app id must be greater than zero",
        ));
    }
    let app = algod
        .application_info(registry_app_id)
        .await
        .map_err(|error| {
            bad_request(format!(
                "native AMM registry application {registry_app_id} not found: {error}"
            ))
        })?;
    verify_native_registry_programs(&app, registry_programs)?;
    let genesis_hash = genesis_hash(algod).await?;
    verify_native_registry_state(&app, pool_programs, genesis_hash)
}
