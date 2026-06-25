use std::env;
use std::fs;
use std::path::PathBuf;

use opennodia_core::Address;
use opennodia_node::AlgodClient;
use opennodia_swap::{
    assign_group_id, build_asset_transfer, build_payment, encode_signed_tx, fetch_tx_params,
    SignedTransaction,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let algod_url =
        env::var("OPENNODIA_ALGOD_URL").unwrap_or_else(|_| "http://127.0.0.1:4001".to_string());
    let token = fs::read_to_string(env::var("OPENNODIA_ALGOD_TOKEN_FILE")?)?
        .trim()
        .to_string();
    let sender: Address = env::var("OPENNODIA_VALIDATION_OWNER")?.parse()?;
    let receiver: Address = env::var("OPENNODIA_VALIDATION_RECEIVER")?.parse()?;
    let asset_id: u64 = env::var("OPENNODIA_VALIDATION_SELL_ASSET")?.parse()?;
    let output_dir = PathBuf::from(env::var("OPENNODIA_VALIDATION_OUTPUT_DIR")?);
    fs::create_dir_all(&output_dir)?;

    let algod = AlgodClient::new(algod_url, token);
    let params = fetch_tx_params(&algod).await?;
    let mut transactions = vec![
        build_payment(sender, receiver, 1, &params),
        build_asset_transfer(sender, receiver, asset_id, 1, &params),
    ];

    let ungrouped: Vec<u8> = transactions
        .iter()
        .cloned()
        .flat_map(|transaction| {
            encode_signed_tx(&SignedTransaction {
                transaction,
                sig: None,
                lsig: None,
            })
        })
        .collect();
    let group_id = assign_group_id(&mut transactions);
    let grouped: Vec<u8> = transactions
        .iter()
        .cloned()
        .flat_map(|transaction| {
            encode_signed_tx(&SignedTransaction {
                transaction,
                sig: None,
                lsig: None,
            })
        })
        .collect();

    fs::write(output_dir.join("ungrouped.tx"), ungrouped)?;
    fs::write(output_dir.join("rust-grouped.tx"), grouped)?;
    println!("group_id={}", hex::encode(group_id));
    Ok(())
}
