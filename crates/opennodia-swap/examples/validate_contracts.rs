use std::env;
use std::fs;

use opennodia_core::Address;
use opennodia_node::AlgodClient;
use opennodia_swap::{EscrowAccount, EscrowKind, EscrowParams};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let algod_url =
        env::var("OPENNODIA_ALGOD_URL").unwrap_or_else(|_| "http://127.0.0.1:4001".to_string());
    let token_file = env::var("OPENNODIA_ALGOD_TOKEN_FILE")?;
    let token = fs::read_to_string(token_file)?.trim().to_string();
    let owner: Address = env::var("OPENNODIA_VALIDATION_OWNER")?.parse()?;
    let sell_asset: u64 = env::var("OPENNODIA_VALIDATION_SELL_ASSET")?.parse()?;
    let buy_asset: u64 = env::var("OPENNODIA_VALIDATION_BUY_ASSET")?.parse()?;
    let expire_round: u64 = env::var("OPENNODIA_VALIDATION_EXPIRE_ROUND")?.parse()?;
    let algod = AlgodClient::new(algod_url, token);

    let cases = [
        (
            "asa-for-algo-sell-label",
            EscrowKind::Sell,
            EscrowParams::new(owner, sell_asset, 1, 0, 1, expire_round),
        ),
        (
            "asa-for-algo-buy-label",
            EscrowKind::Buy,
            EscrowParams::new(owner, sell_asset, 1, 0, 1, expire_round),
        ),
        (
            "algo-for-asa",
            EscrowKind::Buy,
            EscrowParams::new(owner, 0, 1, buy_asset, 1, expire_round),
        ),
        (
            "asa-for-asa",
            EscrowKind::Sell,
            EscrowParams::new(owner, sell_asset, 1, buy_asset, 1, expire_round),
        ),
    ];

    for (name, kind, params) in cases {
        let escrow = EscrowAccount::compile(&algod, kind, params).await?;
        println!(
            "{name}: address={}, program_bytes={}",
            escrow.address,
            escrow.program.len()
        );
    }
    Ok(())
}
