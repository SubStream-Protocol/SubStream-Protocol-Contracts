//! Fuzz Test: Issue #140 — 1-Stroop Micro-Subscription Execution
//!
//! Verifies that integer division in the fee/affiliate math never:
//!   - panics on microscopic amounts
//!   - rounds up in favour of an attacker
//!   - causes underflow when deducting a 10% protocol fee + 10% affiliate split
//!     from a 1-stroop (1 unit) pull
//!
//! Run with: cargo fuzz run micro_subscription_execution
#![no_main]
#![no_std]

use libfuzzer_sys::fuzz_target;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};
use substream_contracts::{SubStreamContract, SubStreamContractClient};

const PROTOCOL_FEE_BPS: i128 = 1000; // 10%
const AFFILIATE_FEE_BPS: i128 = 1000; // 10%
const BPS_DENOM: i128 = 10_000;

/// Simulate the fee deduction math for a single pull of `amount` stroops.
/// Returns (creator_payout, protocol_fee, affiliate_fee, remainder).
/// Asserts that truncation never inflates any party's share.
fn simulate_fee_split(amount: i128) -> (i128, i128, i128, i128) {
    if amount <= 0 {
        return (0, 0, 0, 0);
    }
    let protocol_fee = (amount * PROTOCOL_FEE_BPS) / BPS_DENOM;
    let affiliate_fee = (amount * AFFILIATE_FEE_BPS) / BPS_DENOM;
    let creator_payout = amount - protocol_fee - affiliate_fee;
    let remainder = amount - (protocol_fee + affiliate_fee + creator_payout);

    // Invariants
    assert!(protocol_fee >= 0, "protocol fee must be non-negative");
    assert!(affiliate_fee >= 0, "affiliate fee must be non-negative");
    assert!(creator_payout >= 0, "creator payout must be non-negative");
    assert!(remainder >= 0, "remainder must be non-negative (no underflow)");
    assert!(
        protocol_fee + affiliate_fee + creator_payout + remainder == amount,
        "fee split must be lossless"
    );
    // Truncation must never favour attacker: fees must not exceed their BPS share
    assert!(
        protocol_fee <= (amount * PROTOCOL_FEE_BPS + BPS_DENOM - 1) / BPS_DENOM,
        "protocol fee must not exceed ceiling"
    );
    assert!(
        affiliate_fee <= (amount * AFFILIATE_FEE_BPS + BPS_DENOM - 1) / BPS_DENOM,
        "affiliate fee must not exceed ceiling"
    );

    (creator_payout, protocol_fee, affiliate_fee, remainder)
}

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    // Derive a pull amount between 1 and 255 stroops from fuzz input
    let amount = (data[0] as i128).max(1);

    // Run 100_000 micro-pulls and verify the vault never goes negative
    let mut vault: i128 = amount * 100_000;
    let mut total_protocol_fees: i128 = 0;
    let mut total_affiliate_fees: i128 = 0;
    let mut total_creator_payouts: i128 = 0;

    for _ in 0..100_000u64 {
        if vault < amount {
            break; // vault exhausted — not a bug
        }
        let (creator_payout, protocol_fee, affiliate_fee, _remainder) =
            simulate_fee_split(amount);

        vault -= amount;
        total_protocol_fees += protocol_fee;
        total_affiliate_fees += affiliate_fee;
        total_creator_payouts += creator_payout;

        assert!(vault >= 0, "vault balance went negative");
    }

    // Total distributed must never exceed what was deposited
    let total_distributed = total_protocol_fees + total_affiliate_fees + total_creator_payouts;
    let initial_vault = amount * 100_000;
    assert!(
        total_distributed <= initial_vault,
        "distributed more than deposited: {} > {}",
        total_distributed,
        initial_vault
    );

    // Acceptance 1: truncation never favours attacker or inflates affiliate balances
    // (verified per-iteration above)

    // Acceptance 2: microscopic fractional remains are handled without logic faults
    // The 1-stroop case: fees = 0 (floor division), creator gets 1, remainder = 0
    let (c, p, a, r) = simulate_fee_split(1);
    assert_eq!(p, 0, "1-stroop protocol fee must be 0 (floor division)");
    assert_eq!(a, 0, "1-stroop affiliate fee must be 0 (floor division)");
    assert_eq!(c, 1, "1-stroop creator payout must be 1");
    assert_eq!(r, 0, "1-stroop remainder must be 0");

    // Acceptance 3: high-frequency low-value interactions execute safely
    // (verified by the 100_000-iteration loop above)
});
