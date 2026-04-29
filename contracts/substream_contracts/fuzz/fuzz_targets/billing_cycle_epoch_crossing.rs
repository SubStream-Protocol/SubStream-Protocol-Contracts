//! Fuzz target — billing cycles that cross ledger epoch boundaries.
//!
//! Exercises the streaming-billing collect path (and the cancel-velocity
//! bucket bookkeeping it interacts with) under timestamps that fall on, just
//! before, and just after the contract's epoch boundaries:
//!
//!   * `HOUR_IN_SECONDS` (3600s) — hourly cancel-velocity buckets (24-wide)
//!   * `DAY_IN_SECONDS`  (86400s) — daily cancel-velocity buckets (30-wide)
//!   * `FREE_TRIAL_DURATION` (7 days) — trial → paid-tier transition
//!   * `MINIMUM_FLOW_DURATION` (1 day) — early-cancel penalty cliff
//!
//! Invariants enforced on every fuzz iteration:
//!
//!   1. Token conservation is preserved across subscribe → N collects → cancel.
//!   2. Cumulative creator earnings equal `rate * paid_seconds / PRECISION`,
//!      regardless of how many hour/day boundaries the cycle straddles.
//!   3. Earnings are monotonically non-decreasing across consecutive collects.
//!   4. The hourly/daily cancel-velocity bucket vectors retain their fixed
//!      cardinalities (24 / 30) after a boundary-aligned cancellation.
//!   5. A cancellation timestamped on a fresh hour boundary is reflected in
//!      the rolling-24h and trailing-30d counters.
//!
//! Run with: `cargo fuzz run billing_cycle_epoch_crossing`.

#![no_main]

use libfuzzer_sys::fuzz_target;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{token, Address, Env, String};

use substream_contracts::{
    CancelVelocityMetrics, SubStreamContract, SubStreamContractClient,
};

const HOUR: u64 = 60 * 60;
const DAY: u64 = 24 * HOUR;
const WEEK: u64 = 7 * DAY;

// Mirror the constants the contract uses internally so the assertions below
// remain self-checking even if the lib reorganises its constants.
const FREE_TRIAL_DURATION: u64 = WEEK;
const MINIMUM_FLOW_DURATION: u64 = DAY;
const PRECISION_MULTIPLIER: i128 = 1_000_000_000;

// SEP-12 KYC issuer placeholder used throughout the existing fuzz suite.
const SEP12_ISSUER: &str =
    "GD5DQX2K7Q4D4PE4R6J4Y7Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2";

// Boundary-jitter table: each entry is the signed offset (in seconds) applied
// to a freshly-computed hour boundary. The mix covers exact alignment, "one
// tick before/after" the boundary (the off-by-one zone), and minute-scale
// drift that should not perturb the per-second linear charge.
const BOUNDARY_OFFSETS: [i64; 11] =
    [-3600, -300, -60, -3, -1, 0, 1, 3, 60, 300, 3600];

fuzz_target!(|data: &[u8]| {
    if data.len() < 8 {
        return;
    }

    // ── Test fixture ───────────────────────────────────────────────────────
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let subscriber = Address::generate(&env);
    let issuer = Address::from_string(&String::from_str(&env, SEP12_ISSUER));

    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token_addr = sac.address();
    let token_client = token::Client::new(&env, &token_addr);
    let token_admin = token::StellarAssetClient::new(&env, &token_addr);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);
    client.initialize(&admin);
    client.register_merchant_with_kyc(
        &merchant,
        &soroban_sdk::vec![&env, 1u8],
        &issuer,
    );

    // ── Fuzz parameters ────────────────────────────────────────────────────
    // Whole-token-per-second rate in [1, 8] keeps the cumulative charge well
    // below `deposit` for all reachable cycle counts/lengths below.
    let rate_tokens_per_sec: i128 = 1 + (data[0] as i128 % 8);
    let rate: i128 = rate_tokens_per_sec * PRECISION_MULTIPLIER;
    // Number of collect cycles in [1, 6].
    let num_cycles: usize = 1 + (data[1] as usize % 6);
    // Cycle length in [1, 36] hours so cycles routinely cross both hour and
    // day epoch boundaries.
    let cycle_hours: u64 = 1 + (data[2] as u64 % 36);
    let cycle_secs: u64 = cycle_hours * HOUR;
    // Pick an offset describing how the next collect lands relative to a
    // fresh hour boundary: exact, ε-before, ε-after, or ±minute drift.
    let offset = BOUNDARY_OFFSETS[(data[3] as usize) % BOUNDARY_OFFSETS.len()];
    // Optional shift of the start anchor by 0..23 hours so that day boundaries
    // do not always coincide with hour boundary 0.
    let start_hour_shift: u64 = (data[4] as u64) % 24;

    // Deposit must dominate the maximum total charge across the fuzzed
    // parameter space to keep `last_funds_exhausted` zero and guarantee the
    // linearity invariant. Worst case: 8 tokens/sec × 6 cycles × 36h × 3600s
    // = 6_220_800 tokens. 50M leaves a comfortable margin.
    let deposit: i128 = 50_000_000;
    let mint_total = deposit;
    token_admin.mint(&subscriber, &deposit);

    // Anchor the subscription to a fresh hour boundary plus a fuzz-controlled
    // hour offset, then push it well clear of timestamp 0 so all downstream
    // arithmetic remains in normal-range u64 territory.
    let raw_start: u64 = 10 * DAY + start_hour_shift * HOUR;
    let aligned_start = (raw_start / HOUR) * HOUR;
    env.ledger().set_timestamp(aligned_start);

    // ── Subscribe ──────────────────────────────────────────────────────────
    client.subscribe(
        &subscriber,
        &merchant,
        &token_addr,
        &deposit,
        &rate,
        &None,
    );

    // Token conservation immediately after subscribe (escrow has the deposit).
    assert_eq!(
        token_client.balance(&subscriber)
            + token_client.balance(&merchant)
            + token_client.balance(&contract_id),
        mint_total,
        "token conservation violated immediately after subscribe",
    );

    let trial_end = aligned_start.saturating_add(FREE_TRIAL_DURATION);
    let mut prev_earned: i128 = 0;
    let mut prev_time = aligned_start;

    // ── N billing cycles, each crossing at least one hour boundary ────────
    for i in 0..num_cycles {
        // Per-cycle additional jitter sourced from later fuzz bytes — the
        // outer `offset` plus a small in-bounds wobble keeps the search space
        // dense around each boundary.
        let jitter_byte = data[5 + (i % (data.len() - 5))] as i64;
        let cycle_offset = offset + ((jitter_byte % 7) - 3);

        // Land the next collect on the hour boundary that follows
        // `prev_time + cycle_secs`, then apply the boundary jitter.
        let nominal = prev_time.saturating_add(cycle_secs);
        let next_hour_boundary = ((nominal / HOUR) + 1).saturating_mul(HOUR);
        let candidate = next_hour_boundary as i128 + cycle_offset as i128;
        // Guarantee strict forward progress and that we are past the trial so
        // the linearity assertion has a non-trivial elapsed window.
        let lower_bound = (prev_time + 1).max(trial_end + 1) as i128;
        let next_time: u64 = candidate.max(lower_bound) as u64;

        env.ledger().set_timestamp(next_time);
        client.collect(&subscriber, &merchant);

        // Invariant 2 — earnings linearity across all crossed boundaries.
        // Within the first 6 months the loyalty discount is 0%, so the
        // discounted-charge math reduces to `rate × elapsed_paid_secs`. We
        // bound `next_time` well under SIX_MONTHS to keep this true.
        let earned = token_client.balance(&merchant);
        let paid_elapsed = next_time.saturating_sub(trial_end) as i128;
        let expected = (rate.saturating_mul(paid_elapsed)) / PRECISION_MULTIPLIER;
        assert_eq!(
            earned, expected,
            "earnings drift at cycle {} (next_time={}, expected={}, got={})",
            i, next_time, expected, earned,
        );

        // Invariant 3 — monotonic non-decreasing earnings.
        assert!(
            earned >= prev_earned,
            "earnings decreased across collect: {} -> {}",
            prev_earned,
            earned,
        );

        // Invariant 1 — token conservation across each cycle.
        let bal_sum = token_client.balance(&subscriber)
            + token_client.balance(&merchant)
            + token_client.balance(&contract_id);
        assert_eq!(
            bal_sum, mint_total,
            "token conservation violated after collect at t={}",
            next_time,
        );

        prev_earned = earned;
        prev_time = next_time;
    }

    // ── Cancel exactly on a fresh hour boundary ───────────────────────────
    // The cancel timestamp must be ≥ start + MINIMUM_FLOW_DURATION to avoid
    // the early-cancel penalty path; the fuzzer's parameter ranges already
    // ensure prev_time clears that cliff, but assert it for safety.
    let earliest_cancel = aligned_start.saturating_add(MINIMUM_FLOW_DURATION + 1);
    let cancel_anchor = ((prev_time / HOUR) + 1).saturating_mul(HOUR);
    let cancel_time = cancel_anchor.max(earliest_cancel);
    env.ledger().set_timestamp(cancel_time);
    client.cancel(&subscriber, &merchant);

    let metrics: CancelVelocityMetrics = client.get_cancel_velocity_metrics();

    // Invariant 4 — bucket vectors stay at their fixed cardinalities after a
    // boundary-aligned cancellation. Drift here would indicate state-corruption
    // in the bucket-rotation arithmetic at the epoch boundary.
    assert_eq!(
        metrics.hourly_bucket_count, 24,
        "hourly bucket count must remain 24 after epoch-boundary cancel",
    );
    assert_eq!(
        metrics.daily_bucket_count, 30,
        "daily bucket count must remain 30 after epoch-boundary cancel",
    );

    // Invariant 5 — the just-issued cancellation must be visible in both the
    // rolling-24h and trailing-30d windows (both are inclusive of the current
    // hour/day epoch).
    assert!(
        metrics.rolling_24h_cancellations >= 1,
        "rolling-24h counter must reflect the boundary-aligned cancellation",
    );
    assert!(
        metrics.trailing_30d_cancellations >= 1,
        "trailing-30d counter must reflect the boundary-aligned cancellation",
    );

    // A single cancel cannot exceed the cold-start anomaly threshold
    // (`CANCEL_VELOCITY_MIN_TRIGGER` = 25), so the breaker must stay armed
    // and the protocol must not be soft-paused from this fixture alone.
    assert!(
        !metrics.circuit_breaker_active,
        "single cancellation must not trip the velocity circuit breaker",
    );
    assert!(
        !metrics.soft_pause_active,
        "single cancellation must not soft-pause the protocol",
    );

    // Invariant 1 (final form) — token conservation across the full lifecycle.
    let final_sum = token_client.balance(&subscriber)
        + token_client.balance(&merchant)
        + token_client.balance(&contract_id);
    assert_eq!(
        final_sum, mint_total,
        "token conservation violated after cancel at t={}",
        cancel_time,
    );
});
