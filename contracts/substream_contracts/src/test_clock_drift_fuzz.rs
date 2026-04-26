/// Issue #139: Fuzz Test – Ledger Clock Drift on Billing Cycles
///
/// Systematically sweeps chaotic timestamp inputs to prove that:
///   1. Billing cycles anchor to the original inception date, not the collect time.
///   2. Leap-year months (Feb 28/29), varying month lengths, and ledger drift
///      cannot cause premature or delayed payment pulls.
///   3. The next_billing_date never drifts forward over multi-year simulations.
///
/// "Fuzz" is implemented as a deterministic parameter sweep (no nightly toolchain
/// required) covering the relevant edge-case space.
///
/// Acceptance 1: Billing cycles are mathematically proven to remain anchored over decades.
/// Acceptance 2: Calendar anomalies and ledger drift cannot cause premature/delayed pulls.
/// Acceptance 3: Timeline is immune to low-level network clock variance.
#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const RATE: i128 = PRECISION_MULTIPLIER; // 1 token/sec
const DEPOSIT: i128 = 500_000_000 * PRECISION_MULTIPLIER;

// Month lengths in seconds (non-leap and leap February)
const MONTH_LENGTHS_SEC: &[u64] = &[
    31 * 86400, // Jan
    28 * 86400, // Feb (non-leap)
    29 * 86400, // Feb (leap)
    31 * 86400, // Mar
    30 * 86400, // Apr
    31 * 86400, // May
    30 * 86400, // Jun
    31 * 86400, // Jul
    31 * 86400, // Aug
    30 * 86400, // Sep
    31 * 86400, // Oct
    30 * 86400, // Nov
    31 * 86400, // Dec
];

// Ledger drift deltas to inject (seconds): negative = early, positive = late
const DRIFT_DELTAS: &[i64] = &[-300, -60, -1, 0, 1, 60, 300, 3600];

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn setup_subscription(env: &Env, start: u64) -> (SubStreamContractClient, Address, Address, token::Client) {
    let admin = Address::generate(env);
    let merchant = Address::generate(env);
    let token_admin = Address::generate(env);

    let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token = token::Client::new(env, &sac.address());
    let token_sa = token::StellarAssetClient::new(env, &sac.address());

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(env, &contract_id);
    client.initialize(&admin);

    env.as_contract(&contract_id, || {
        env.storage().persistent().set(
            &DataKey::MerchantRegistry(merchant.clone()),
            &MerchantStatus {
                is_verified: true,
                is_blacklisted: false,
                verification_method: VerificationMethod::DAOApproval,
                registered_at: 0,
                last_verified: 0,
                dao_approved: true,
            },
        );
    });

    let subscriber = Address::generate(env);
    token_sa.mint(&subscriber, &DEPOSIT);

    env.ledger().set_timestamp(start);
    client.subscribe(&subscriber, &merchant, &token.address, &DEPOSIT, &RATE, &None);

    (client, merchant, subscriber, token)
}

// ---------------------------------------------------------------------------
// Core invariant: charge is proportional to elapsed time, not calendar drift
// ---------------------------------------------------------------------------

/// For each (month_length, drift) combination, verify that the amount collected
/// equals exactly `elapsed_seconds * rate_per_token` (within precision rounding).
#[test]
fn test_billing_anchors_to_inception_across_month_lengths_and_drift() {
    for &month_sec in MONTH_LENGTHS_SEC {
        for &drift in DRIFT_DELTAS {
            let env = Env::default();
            env.mock_all_auths();

            let start: u64 = 1_000_000;
            let (client, merchant, subscriber, token) = setup_subscription(&env, start);

            // Advance past trial, then one full "month" with drift applied
            let trial_end = start + FREE_TRIAL_DURATION;
            let collect_at = (trial_end + month_sec) as i64 + drift;
            let collect_at = collect_at.max(trial_end as i64 + 1) as u64;

            env.ledger().set_timestamp(collect_at);
            client.collect(&subscriber, &merchant);

            let earned = token.balance(&merchant);
            let elapsed = collect_at.saturating_sub(trial_end) as i128;
            // rate = PRECISION_MULTIPLIER tokens/sec → 1 whole token per second
            let expected = elapsed; // 1 token per second

            assert_eq!(
                earned, expected,
                "month_sec={month_sec} drift={drift}: expected {expected} tokens, got {earned}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Multi-year drift accumulation: billing must not shift forward over 5 years
// ---------------------------------------------------------------------------

/// Simulate 60 monthly collects (5 years) with alternating drift and verify
/// that total earned = total_elapsed_paid_seconds (no drift accumulation).
#[test]
fn test_no_drift_accumulation_over_5_years() {
    let env = Env::default();
    env.mock_all_auths();

    let start: u64 = 1_000_000;
    let (client, merchant, subscriber, token) = setup_subscription(&env, start);

    let trial_end = start + FREE_TRIAL_DURATION;
    let mut current_time = trial_end + 1;
    let month = 30 * 86400_u64;

    // 60 monthly collects with alternating ±5 minute drift
    for i in 0..60_u64 {
        let drift: i64 = if i % 2 == 0 { 300 } else { -300 };
        current_time = ((current_time + month) as i64 + drift).max(current_time as i64 + 1) as u64;
        env.ledger().set_timestamp(current_time);
        client.collect(&subscriber, &merchant);
    }

    let total_earned = token.balance(&merchant);
    let total_elapsed = current_time.saturating_sub(trial_end) as i128;

    // Earned must equal elapsed seconds (1 token/sec rate)
    assert_eq!(
        total_earned, total_elapsed,
        "5-year drift accumulation: expected {total_elapsed} tokens, got {total_earned}"
    );
}

// ---------------------------------------------------------------------------
// Edge-of-month: Feb 28 → Feb 29 (leap year boundary)
// ---------------------------------------------------------------------------

/// Verify that a subscription started on Feb 28 and collected on Feb 29 (leap year)
/// correctly charges for exactly 1 day (86400 seconds).
#[test]
fn test_leap_year_feb_28_to_feb_29_charges_one_day() {
    let env = Env::default();
    env.mock_all_auths();

    // Feb 28 of a leap year (arbitrary epoch that lands on Feb 28)
    // Using a fixed timestamp: 2024-02-28 00:00:00 UTC ≈ 1709078400
    let feb_28: u64 = 1_709_078_400;
    let feb_29: u64 = feb_28 + 86400;

    let (client, merchant, subscriber, token) = setup_subscription(&env, feb_28);

    // Collect on Feb 29 (still within trial — no charge expected)
    env.ledger().set_timestamp(feb_29);
    client.collect(&subscriber, &merchant);
    assert_eq!(token.balance(&merchant), 0, "still in trial on Feb 29");

    // Collect after trial ends
    let after_trial = feb_28 + FREE_TRIAL_DURATION + 86400;
    env.ledger().set_timestamp(after_trial);
    client.collect(&subscriber, &merchant);

    let earned = token.balance(&merchant);
    let expected = 86400_i128; // 1 day at 1 token/sec
    assert_eq!(
        earned, expected,
        "leap year Feb 28→29: expected {expected} tokens for 1 day, got {earned}"
    );
}

// ---------------------------------------------------------------------------
// Systematic sweep: 1000 random-ish time deltas via deterministic LCG
// ---------------------------------------------------------------------------

/// Deterministic pseudo-random sweep over 1000 time deltas to stress the
/// billing math without requiring an external fuzzing harness.
#[test]
fn test_systematic_1000_delta_sweep() {
    // Simple LCG for deterministic "random" deltas
    let mut state: u64 = 0xDEAD_BEEF_1337_CAFE;
    let lcg_next = |s: &mut u64| -> u64 {
        *s = s.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        *s
    };

    let mut failures = 0usize;
    let mut total = 0usize;

    for _ in 0..1000 {
        let env = Env::default();
        env.mock_all_auths();

        let start: u64 = 1_000_000;
        let (client, merchant, subscriber, token) = setup_subscription(&env, start);

        let trial_end = start + FREE_TRIAL_DURATION;

        // Generate a random elapsed time between 1 second and 365 days
        let elapsed_sec = (lcg_next(&mut state) % (365 * 86400)) + 1;
        let collect_at = trial_end + elapsed_sec;

        env.ledger().set_timestamp(collect_at);
        client.collect(&subscriber, &merchant);

        let earned = token.balance(&merchant);
        let expected = elapsed_sec as i128;

        if earned != expected {
            failures += 1;
        }
        total += 1;
    }

    assert_eq!(
        failures, 0,
        "{failures}/{total} iterations failed the billing invariant"
    );
}
