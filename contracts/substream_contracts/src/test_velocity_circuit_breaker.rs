#![cfg(test)]

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{token, vec, Address, Env};

fn seed_subscription(
    env: &Env,
    subscriber: &Address,
    creator: &Address,
    token_address: &Address,
    balance_tokens: i128,
) {
    let now = env.ledger().timestamp();
    let key = subscription_key(subscriber, creator);
    let sub = Subscription {
        token: token_address.clone(),
        tier: Tier {
            rate_per_second: 0,
            trial_duration: 0,
        },
        balance: balance_tokens * PRECISION_MULTIPLIER,
        last_collected: now,
        start_time: now,
        streak_start_date: now,
        last_funds_exhausted: 0,
        flags: 0,
        creators: vec![env, creator.clone()],
        percentages: vec![env, 100u32],
        payer: subscriber.clone(),
        beneficiary: subscriber.clone(),
        accrued_remainder: 0,
    };

    set_subscription(env, &key, &sub);
    env.storage()
        .persistent()
        .set(&DataKey::CurrentFlowRate(creator.clone()), &0i128);
}

fn create_token_contract<'a>(env: &Env, admin: &Address) -> token::Client<'a> {
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    token::Client::new(env, &sac.address())
}

#[test]
fn breaker_triggers_when_24h_velocity_exceeds_threshold() {
    let env = Env::default();
    env.mock_all_auths();

    for day in 0..30u64 {
        env.ledger()
            .set_timestamp(day.saturating_mul(DAY_IN_SECONDS).saturating_add(10));
        let metrics = record_protocol_cancellation(&env);
        assert!(!metrics.circuit_breaker_active);
    }

    let baseline = sync_cancel_velocity_metrics(&env);
    assert_eq!(baseline.daily_average_30d, 1);
    assert_eq!(baseline.anomaly_threshold, CANCEL_VELOCITY_MIN_TRIGGER);

    env.ledger()
        .set_timestamp(29u64.saturating_mul(DAY_IN_SECONDS).saturating_add(100));

    for _ in 0..24 {
        let metrics = record_protocol_cancellation(&env);
        assert!(!metrics.circuit_breaker_active);
    }

    let triggered = record_protocol_cancellation(&env);
    assert!(triggered.circuit_breaker_active);
    assert!(triggered.soft_pause_active);
    assert_eq!(triggered.rolling_24h_cancellations, 26);
    assert_eq!(triggered.anomaly_threshold, CANCEL_VELOCITY_MIN_TRIGGER);
}

#[test]
fn normal_usage_does_not_false_positive() {
    let env = Env::default();
    env.mock_all_auths();

    for day in 0..45u64 {
        env.ledger()
            .set_timestamp(day.saturating_mul(DAY_IN_SECONDS).saturating_add(10));
        let metrics = record_protocol_cancellation(&env);
        assert!(!metrics.circuit_breaker_active);
    }

    let metrics = sync_cancel_velocity_metrics(&env);
    assert_eq!(metrics.daily_average_30d, 1);
    assert!(!metrics.circuit_breaker_active);
    assert_eq!(metrics.hourly_bucket_count, CANCEL_VELOCITY_HOURLY_BUCKETS);
    assert_eq!(metrics.daily_bucket_count, CANCEL_VELOCITY_DAILY_BUCKETS);
}

#[test]
fn soft_pause_blocks_top_up_but_cancel_still_works() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&subscriber, &100);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    env.ledger().set_timestamp(1_000);
    seed_subscription(&env, &subscriber, &creator, &token.address, 10);

    let mut state = read_velocity_circuit_breaker_state(&env);
    state.active = true;
    state.soft_pause_active = true;
    write_velocity_circuit_breaker_state(&env, &state);

    let top_up_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.top_up(&subscriber, &creator, &1);
    }));
    assert!(top_up_result.is_err());

    client.cancel(&subscriber, &creator);
    assert!(!subscription_exists(&env, &subscription_key(&subscriber, &creator)));
}

#[test]
fn admin_can_reset_breaker_and_recover_protocol() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    let mut state = read_velocity_circuit_breaker_state(&env);
    state.active = true;
    state.soft_pause_active = true;
    state.triggered_at = 77;
    write_velocity_circuit_breaker_state(&env, &state);

    env.ledger().set_timestamp(2_000);
    client.reset_cancel_velocity_circuit_breaker(&admin);

    let metrics = client.get_cancel_velocity_metrics();
    assert!(!metrics.circuit_breaker_active);
    assert!(!metrics.soft_pause_active);
    assert_eq!(metrics.triggered_at, 0);
}

#[test]
fn storage_remains_bounded_after_long_running_activity() {
    let env = Env::default();
    env.mock_all_auths();

    for day in 0..120u64 {
        env.ledger()
            .set_timestamp(day.saturating_mul(DAY_IN_SECONDS).saturating_add(10));
        for _ in 0..3 {
            record_protocol_cancellation(&env);
        }
    }

    let metrics = sync_cancel_velocity_metrics(&env);
    assert_eq!(metrics.hourly_bucket_count, CANCEL_VELOCITY_HOURLY_BUCKETS);
    assert_eq!(metrics.daily_bucket_count, CANCEL_VELOCITY_DAILY_BUCKETS);
    assert!(metrics.trailing_30d_cancellations <= 90);
}
