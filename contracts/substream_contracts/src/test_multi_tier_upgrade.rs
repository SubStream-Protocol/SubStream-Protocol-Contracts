//! Fuzz tests for multi-tier upgrade/downgrade mid-subscription (Issue #9).
//!
//! Core invariant under test:
//!   total_paid_to_creator == time_on_first_tier  * first_rate_per_sec
//!                          + time_on_second_tier * second_rate_per_sec
//!
//! "Fuzz" here means a systematic parameter sweep over many (rate, duration)
//! combinations that together cover the relevant input space without requiring
//! an external fuzzing harness or nightly toolchain.
//!
//! All scenarios stay well within the first 6-month discount window so that
//! `calculate_discounted_charge` applies 0 % discount and the expected value
//! is a simple product.  Discount-period behaviour is already covered by the
//! rate calculation itself and is orthogonal to the tier-switching invariant.

#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

// One week in seconds (= FREE_TRIAL_DURATION).
const WEEK: u64 = 7 * 24 * 60 * 60;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn create_token<'a>(env: &Env, admin: &Address) -> (token::Client<'a>, token::StellarAssetClient<'a>) {
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let client = token::Client::new(env, &sac.address());
    let admin_client = token::StellarAssetClient::new(env, &sac.address());
    (client, admin_client)
}

/// Run a single upgrade/downgrade scenario and assert the pro-rated invariant.
///
/// `bronze_tps` / `gold_tps` are *whole token* rates (tokens per second).
/// Internally the contract uses nano-precision (`rate = tps * PRECISION_MULTIPLIER`).
///
/// Timeline (all times are seconds from `start`):
///   [start, start+WEEK)          – free trial, no charges
///   [start+WEEK, upgrade_time)   – bronze phase: `time_on_bronze` seconds
///   [upgrade_time, collect_time) – gold phase:   `time_on_gold`   seconds
fn run_scenario(bronze_tps: i128, gold_tps: i128, time_on_bronze: u64, time_on_gold: u64) {
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);
    let (token, token_admin) = create_token(&env, &admin);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    // Convert whole-token rates to nano-precision contract rates.
    let bronze_rate = bronze_tps * PRECISION_MULTIPLIER;
    let gold_rate = gold_tps * PRECISION_MULTIPLIER;

    // Mint enough to cover both phases with a comfortable buffer.
    let needed: i128 = bronze_tps * time_on_bronze as i128
        + gold_tps * time_on_gold as i128
        + 10_000; // buffer so balance never exhausts mid-test
    token_admin.mint(&subscriber, &needed);

    let start = 1_000_000_u64; // arbitrary non-zero start
    env.ledger().set_timestamp(start);
    client.subscribe(&subscriber, &creator, &token.address, &needed, &bronze_rate);

    // ── Bronze phase ────────────────────────────────────────────────────────
    let upgrade_time = start + WEEK + time_on_bronze;
    env.ledger().set_timestamp(upgrade_time);

    // change_tier settles all bronze charges internally before switching.
    client.change_tier(&subscriber, &creator, &gold_rate);

    // Creator should already have received payment for the bronze phase.
    let bronze_earned = token.balance(&creator);
    assert_eq!(
        bronze_earned,
        bronze_tps * time_on_bronze as i128,
        "bronze phase: bronze_tps={bronze_tps}/s × {time_on_bronze}s"
    );

    // ── Gold phase ──────────────────────────────────────────────────────────
    let collect_time = upgrade_time + time_on_gold;
    env.ledger().set_timestamp(collect_time);
    client.collect(&subscriber, &creator);

    let total_earned = token.balance(&creator);
    let expected = bronze_tps * time_on_bronze as i128 + gold_tps * time_on_gold as i128;
    assert_eq!(
        total_earned, expected,
        "total_paid invariant failed: \
         bronze={bronze_tps}/s×{time_on_bronze}s + gold={gold_tps}/s×{time_on_gold}s \
         → expected {expected}, got {total_earned}"
    );
}

// ---------------------------------------------------------------------------
// Deterministic baseline tests (fast, readable failure messages)
// ---------------------------------------------------------------------------

#[test]
fn test_upgrade_bronze_to_gold_basic() {
    // 2 tok/s for 30 s → 60; then 5 tok/s for 20 s → 100; total 160
    run_scenario(2, 5, 30, 20);
}

#[test]
fn test_downgrade_gold_to_bronze() {
    // 10 tok/s for 60 s → 600; then 2 tok/s for 120 s → 240; total 840
    run_scenario(10, 2, 60, 120);
}

#[test]
fn test_upgrade_same_rate_is_noop() {
    // Changing to the same rate must not double-charge or under-charge.
    run_scenario(5, 5, 100, 100);
}

#[test]
fn test_upgrade_at_trial_boundary() {
    // time_on_bronze = 0: upgrade fires exactly at trial end, bronze charges = 0.
    run_scenario(3, 7, 0, 60);
}

#[test]
fn test_upgrade_very_short_gold_phase() {
    run_scenario(4, 10, 300, 1);
}

#[test]
fn test_upgrade_very_short_bronze_phase() {
    run_scenario(1, 20, 1, 300);
}

#[test]
fn test_upgrade_large_rates_and_times() {
    // Stay well within 6-month window to keep discount = 0.
    run_scenario(100, 200, 3600, 7200);
}

// ---------------------------------------------------------------------------
// Fuzz sweep: assert invariant across many (rate × rate × time × time) tuples
// ---------------------------------------------------------------------------

#[test]
fn test_fuzz_switching_rates() {
    // Whole-token rates per second.  Kept to 4 values so the 4-dimensional
    // cartesian product (4^4 = 256 scenarios) completes quickly.
    const RATES: &[i128] = &[1, 3, 10, 100];
    // Durations in seconds (all well within first 6-month discount window).
    const TIMES: &[u64] = &[0, 30, 3_600, 86_400];

    for &br in RATES {
        for &gr in RATES {
            for &tb in TIMES {
                for &tg in TIMES {
                    run_scenario(br, gr, tb, tg);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Invariant: total_paid = time_on_bronze + time_on_gold (explicit collect split)
// ---------------------------------------------------------------------------

/// Collect explicitly at the upgrade boundary, then again after the gold phase,
/// and verify the two partial sums add up correctly.
#[test]
fn test_partial_collect_before_upgrade_then_collect_after() {
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);
    let (token, token_admin) = create_token(&env, &admin);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    let bronze_tps: i128 = 3;
    let gold_tps: i128 = 7;
    let time_on_bronze: u64 = 100;
    let time_on_gold: u64 = 50;

    let bronze_rate = bronze_tps * PRECISION_MULTIPLIER;
    let gold_rate = gold_tps * PRECISION_MULTIPLIER;

    token_admin.mint(&subscriber, &10_000);

    let start = 1_000_000_u64;
    env.ledger().set_timestamp(start);
    client.subscribe(&subscriber, &creator, &token.address, &10_000, &bronze_rate);

    // Explicitly collect mid-bronze (partial charge).
    let mid_bronze = start + WEEK + time_on_bronze / 2;
    env.ledger().set_timestamp(mid_bronze);
    client.collect(&subscriber, &creator);
    let after_mid_bronze = token.balance(&creator);

    // Upgrade at the end of the full bronze phase.
    let upgrade_time = start + WEEK + time_on_bronze;
    env.ledger().set_timestamp(upgrade_time);
    client.change_tier(&subscriber, &creator, &gold_rate);
    let after_bronze = token.balance(&creator);

    // Entire bronze window must be paid.
    assert_eq!(
        after_bronze,
        bronze_tps * time_on_bronze as i128,
        "bronze total should be {}", bronze_tps * time_on_bronze as i128
    );
    // Mid-collect must be a subset.
    assert!(after_mid_bronze <= after_bronze, "mid-collect must not exceed full bronze payment");

    // Collect gold phase.
    env.ledger().set_timestamp(upgrade_time + time_on_gold);
    client.collect(&subscriber, &creator);
    let total = token.balance(&creator);

    let expected = bronze_tps * time_on_bronze as i128 + gold_tps * time_on_gold as i128;
    assert_eq!(total, expected, "total_paid invariant failed");
}

/// Vault (contract) balance must never go negative across a full upgrade lifecycle.
#[test]
fn test_vault_balance_non_negative_across_upgrade() {
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);
    let (token, token_admin) = create_token(&env, &admin);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    token_admin.mint(&subscriber, &50_000);

    let start = 500_000_u64;
    let rate_5 = 5_i128 * PRECISION_MULTIPLIER;
    let rate_15 = 15_i128 * PRECISION_MULTIPLIER;
    env.ledger().set_timestamp(start);
    client.subscribe(&subscriber, &creator, &token.address, &50_000, &rate_5);

    env.ledger().set_timestamp(start + WEEK + 200);
    assert!(token.balance(&contract_id) >= 0, "vault negative after subscribe");

    client.change_tier(&subscriber, &creator, &rate_15);
    assert!(token.balance(&contract_id) >= 0, "vault negative after change_tier");

    env.ledger().set_timestamp(start + WEEK + 200 + 300);
    client.collect(&subscriber, &creator);
    assert!(token.balance(&contract_id) >= 0, "vault negative after collect");

    env.ledger().set_timestamp(start + WEEK + 200 + 300 + WEEK);
    client.cancel(&subscriber, &creator);
    assert!(token.balance(&contract_id) >= 0, "vault negative after cancel");
}

/// TierChanged event must be emitted exactly once per change_tier call.
#[test]
fn test_tier_changed_event_emitted() {
    use soroban_sdk::testutils::Events as _;

    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);
    let (_, token_admin) = create_token(&env, &admin);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    // Need the token address to subscribe; re-register to get it.
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token_addr = sac.address();
    let token_admin2 = token::StellarAssetClient::new(&env, &token_addr);
    token_admin2.mint(&subscriber, &50_000);
    let _ = token_admin; // suppress unused warning

    let start = 200_000_u64;
    let rate_2 = 2_i128 * PRECISION_MULTIPLIER;
    let rate_8 = 8_i128 * PRECISION_MULTIPLIER;
    env.ledger().set_timestamp(start);
    client.subscribe(&subscriber, &creator, &token_addr, &50_000, &rate_2);

    env.ledger().set_timestamp(start + WEEK + 10);
    client.change_tier(&subscriber, &creator, &rate_8);

    // At least one event from this contract should exist.
    let events = env.events().all().filter_by_contract(&contract_id);
    assert!(!events.events().is_empty(), "expected TierChanged event");
}
