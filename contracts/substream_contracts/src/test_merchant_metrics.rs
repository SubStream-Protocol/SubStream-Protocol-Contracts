/// Issue #128: Build get_merchant_metrics Read-Only Query
/// Tests covering:
/// Acceptance 1: Merchants can instantly query core business metrics from blockchain.
/// Acceptance 2: Aggregation logic handles large datasets without timing out.
/// Acceptance 3: Separation of Active vs. Dunning users provides payment health insight.
#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup() -> (
    Env,
    SubStreamContractClient<'static>,
    Address, // admin
) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    let client: SubStreamContractClient<'static> = unsafe { core::mem::transmute(client) };
    (env, client, admin)
}

fn register_merchant(env: &Env, client: &SubStreamContractClient, merchant: &Address, contract_id: &Address) {
    let merchant_status = MerchantStatus {
        is_verified: true,
        is_blacklisted: false,
        verification_method: VerificationMethod::DAOApproval,
        registered_at: 0,
        last_verified: 0,
        dao_approved: true,
    };
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::MerchantRegistry(merchant.clone()), &merchant_status);
    });
    let _ = client;
}

// ---------------------------------------------------------------------------
// Acceptance 1: Instant query of core metrics
// ---------------------------------------------------------------------------

#[test]
fn test_get_metrics_returns_zeros_for_new_merchant() {
    let (env, client, _admin) = setup();
    let merchant = Address::generate(&env);

    let metrics = client.get_merchant_metrics(&merchant);
    assert_eq!(metrics.total_subscribers, 0);
    assert_eq!(metrics.active_subscribers, 0);
    assert_eq!(metrics.dunning_subscribers, 0);
    assert_eq!(metrics.total_revenue, 0);
    assert_eq!(metrics.avg_revenue_per_subscriber, 0);
    assert_eq!(metrics.last_updated, 0);
}

#[test]
fn test_metrics_increment_on_subscribe() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    let merchant = Address::generate(&env);
    register_merchant(&env, &client, &merchant, &contract_id);

    let subscriber = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token = token::Client::new(&env, &sac.address());
    let token_admin_client = token::StellarAssetClient::new(&env, &sac.address());
    token_admin_client.mint(&subscriber, &100_000);

    client.subscribe(
        &subscriber,
        &merchant,
        &token.address,
        &100_000,
        &1_000_000_000,
        &None,
    );

    let metrics = client.get_merchant_metrics(&merchant);
    assert_eq!(metrics.total_subscribers, 1);
    assert_eq!(metrics.active_subscribers, 1);
}

#[test]
fn test_metrics_decrement_on_cancel() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(0);

    let admin = Address::generate(&env);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    let merchant = Address::generate(&env);
    register_merchant(&env, &client, &merchant, &contract_id);

    let subscriber = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token = token::Client::new(&env, &sac.address());
    let token_admin_client = token::StellarAssetClient::new(&env, &sac.address());
    token_admin_client.mint(&subscriber, &100_000);

    client.subscribe(
        &subscriber,
        &merchant,
        &token.address,
        &100_000,
        &1_000_000_000,
        &None,
    );

    assert_eq!(client.get_merchant_metrics(&merchant).active_subscribers, 1);

    // Cancel after minimum duration
    env.ledger().set_timestamp(2 * 24 * 60 * 60 + 1);
    client.cancel(&subscriber, &merchant);

    let metrics = client.get_merchant_metrics(&merchant);
    assert_eq!(metrics.active_subscribers, 0);
    // Total_subscribers should still be 1 (lifetime count)
    assert_eq!(metrics.total_subscribers, 1);
}

#[test]
fn test_metrics_revenue_accumulates_on_collect() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(0);

    let admin = Address::generate(&env);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    let merchant = Address::generate(&env);
    register_merchant(&env, &client, &merchant, &contract_id);

    let subscriber = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token = token::Client::new(&env, &sac.address());
    let token_admin_client = token::StellarAssetClient::new(&env, &sac.address());
    token_admin_client.mint(&subscriber, &1_000_000);

    // rate = 1 token/sec
    client.subscribe(
        &subscriber,
        &merchant,
        &token.address,
        &1_000_000,
        &1_000_000_000,
        &None,
    );

    // Advance past free trial (7 days) + 100 seconds
    let free_trial_secs = 7 * 24 * 60 * 60u64;
    env.ledger().set_timestamp(free_trial_secs + 100);
    client.collect(&subscriber, &merchant);

    let metrics = client.get_merchant_metrics(&merchant);
    // After 100 seconds of paid streaming at 1 token/sec → 100 tokens revenue
    assert_eq!(metrics.total_revenue, 100);
}

// ---------------------------------------------------------------------------
// Acceptance 2: update_merchant_metrics handles large values
// ---------------------------------------------------------------------------

#[test]
fn test_update_metrics_handles_large_subscriber_count() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    // Simulate many subscribers via bulk delta update
    client.update_merchant_metrics(&admin, &merchant, &10_000, &0, &0);

    let metrics = client.get_merchant_metrics(&merchant);
    assert_eq!(metrics.active_subscribers, 10_000);
    assert_eq!(metrics.total_subscribers, 10_000);
}

#[test]
fn test_update_metrics_large_revenue() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    // Add one active subscriber first, then large revenue
    client.update_merchant_metrics(&admin, &merchant, &1, &0, &0);
    client.update_merchant_metrics(&admin, &merchant, &0, &0, &1_000_000_000_000i128);

    let metrics = client.get_merchant_metrics(&merchant);
    assert_eq!(metrics.total_revenue, 1_000_000_000_000i128);
    assert_eq!(metrics.avg_revenue_per_subscriber, 1_000_000_000_000i128);
}

#[test]
fn test_update_metrics_sequential_accumulation() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    for i in 1..=100i64 {
        client.update_merchant_metrics(&admin, &merchant, &1, &0, &1000);
        let m = client.get_merchant_metrics(&merchant);
        assert_eq!(m.active_subscribers, i as u64);
        assert_eq!(m.total_subscribers, i as u64);
    }
    let m = client.get_merchant_metrics(&merchant);
    assert_eq!(m.total_revenue, 100 * 1000);
}

// ---------------------------------------------------------------------------
// Acceptance 3: Separation of Active vs. Dunning subscribers
// ---------------------------------------------------------------------------

#[test]
fn test_dunning_subscribers_tracked_separately() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    // Add 5 active, 2 dunning
    client.update_merchant_metrics(&admin, &merchant, &5, &2, &0);

    let metrics = client.get_merchant_metrics(&merchant);
    assert_eq!(metrics.active_subscribers, 5);
    assert_eq!(metrics.dunning_subscribers, 2);
}

#[test]
fn test_dunning_resolution_decrements_dunning_count() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    // 3 active, 2 dunning
    client.update_merchant_metrics(&admin, &merchant, &3, &2, &0);

    // One dunning subscriber recovers
    client.update_merchant_metrics(&admin, &merchant, &0, &-1, &0);

    let metrics = client.get_merchant_metrics(&merchant);
    assert_eq!(metrics.active_subscribers, 3);
    assert_eq!(metrics.dunning_subscribers, 1);
}

#[test]
fn test_avg_revenue_per_subscriber_computed_correctly() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    // 4 active subscribers, 400 total revenue → avg = 100
    client.update_merchant_metrics(&admin, &merchant, &4, &0, &400);

    let metrics = client.get_merchant_metrics(&merchant);
    assert_eq!(metrics.avg_revenue_per_subscriber, 100);
}

#[test]
fn test_avg_revenue_zero_when_no_active_subscribers() {
    let (env, client, admin) = setup();
    let merchant = Address::generate(&env);

    // Revenue but no active subscribers (all cancelled)
    // First add 2 active + revenue
    client.update_merchant_metrics(&admin, &merchant, &2, &0, &200);
    // Then remove both
    client.update_merchant_metrics(&admin, &merchant, &-2, &0, &0);

    let metrics = client.get_merchant_metrics(&merchant);
    assert_eq!(metrics.active_subscribers, 0);
    assert_eq!(metrics.avg_revenue_per_subscriber, 0);
}

#[test]
fn test_last_updated_timestamp_set_on_update() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(12345);

    let admin = Address::generate(&env);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    let merchant = Address::generate(&env);
    client.update_merchant_metrics(&admin, &merchant, &1, &0, &0);

    let metrics = client.get_merchant_metrics(&merchant);
    assert_eq!(metrics.last_updated, 12345);
}

// ---------------------------------------------------------------------------
// Authorization
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "unauthorized metrics update")]
fn test_unauthorized_metrics_update_panics() {
    let (env, client, _admin) = setup();
    let merchant = Address::generate(&env);
    let faker = Address::generate(&env);

    // faker is neither admin nor merchant
    client.update_merchant_metrics(&faker, &merchant, &1, &0, &0);
}

#[test]
fn test_merchant_can_update_own_metrics() {
    let (env, client, _admin) = setup();
    let merchant = Address::generate(&env);

    // merchant updates their own metrics
    client.update_merchant_metrics(&merchant, &merchant, &3, &1, &900);

    let metrics = client.get_merchant_metrics(&merchant);
    assert_eq!(metrics.active_subscribers, 3);
    assert_eq!(metrics.dunning_subscribers, 1);
}
