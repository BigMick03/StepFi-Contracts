use soroban_sdk::contracttype;

/// Pool statistics returned by get_pool_stats
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolStats {
    pub total_liquidity: i128,
    pub locked_liquidity: i128,
    pub available_liquidity: i128,
    pub total_shares: i128,
    /// Share price expressed in basis points (10000 = $1.00)
    pub share_price: i128,
}

// Fee split constants (basis points, sum = 10000)
pub const LP_FEE_BPS: i128 = 8500; // 85% to liquidity providers
pub const PROTOCOL_FEE_BPS: i128 = 1000; // 10% to protocol treasury
pub const MERCHANT_FEE_BPS: i128 = 500; // 5% to merchant incentive fund
pub const TOTAL_BPS: i128 = 10000;

/// Precision used for share price calculation (10000 = 1.0)
pub const SHARE_PRICE_PRECISION: i128 = 10_000;

/// Minimum deposit / withdrawal to prevent rounding exploits
pub const MIN_AMOUNT: i128 = 1;

/// Default maximum fraction of *available* liquidity that may be paid out
/// through `fund_loan` within a single ledger, in basis points.
///
/// Defaults to 10000 (= 100% of available liquidity, i.e. cap effectively
/// disabled) so existing honest flows are unchanged until an admin configures
/// a restrictive value via `set_max_outflow_bps`. A value such as 2500 (25%)
/// bounds how fast a misbehaving creditline can drain the pool per ledger.
pub const DEFAULT_MAX_OUTFLOW_BPS: u32 = 10000;

/// Default maximum cumulative amount that may be funded to a single merchant
/// before the pool rejects further `fund_loan` calls for that address.
///
/// `0` means the concentration cap is disabled (fully open, matching legacy
/// behavior) so existing honest flows are unchanged until an admin configures
/// a ceiling via `set_max_per_merchant`.
pub const DEFAULT_MAX_PER_MERCHANT: i128 = 0;

/// Rolling window used by the per-ledger outflow cap. When the current ledger
/// sequence exceeds `start_ledger`, the window is treated as expired and the
/// accumulated outflow resets.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LedgerOutflowWindow {
    /// Ledger sequence in which this window began.
    pub start_ledger: u32,
    /// Available liquidity snapshot when the window began. The per-ledger
    /// budget is `available × max_outflow_bps / 10000`, fixed for the window.
    pub available: i128,
    /// Cumulative `fund_loan` outflows within the current window.
    pub outflow: i128,
}


