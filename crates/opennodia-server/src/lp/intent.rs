//! Native LP one-time transaction intent payloads.

use opennodia_amm::{AddLiquidityQuote, PoolKey, PoolState, RemoveLiquidityQuote, SwapQuote};

use crate::tx_flow::WalletTxGroup;

#[derive(Debug, Clone)]
pub(crate) enum LpIntentAction {
    RegistryCreate {
        group: WalletTxGroup,
    },
    Create {
        group: WalletTxGroup,
        pool_key: PoolKey,
    },
    Setup {
        group: WalletTxGroup,
        pool_before: PoolState,
    },
    Bootstrap {
        group: WalletTxGroup,
        pool_before: PoolState,
        quote: AddLiquidityQuote,
    },
    Add {
        group: WalletTxGroup,
        pool_before: PoolState,
        quote: AddLiquidityQuote,
    },
    Remove {
        group: WalletTxGroup,
        pool_before: PoolState,
        quote: RemoveLiquidityQuote,
    },
    Swap {
        group: WalletTxGroup,
        pool_before: PoolState,
        quote: SwapQuote,
    },
}
