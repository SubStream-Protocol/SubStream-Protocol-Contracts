//! Tests for Issue #134: Data-Pruning for Canceled Subscriptions
#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

const NINETY_DAYS: u64 = 90 * 24 * 60 * 60;
const MINIMUM_DURATION: u64 = 86400; // 24 hours

fn setup(env: &Env) -> (Address, Address, token::Client, token::StellarAssetClient, Address) {
    let admin = Address::generate(env);
    let contract_id = env.register(SubStreamContract, ());
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token_client = token::Client::new(env, &sac.address());
    let token_admin = token::StellarAssetClient::new(env, &sac.address());
    (admin, contract_id, token_client, token_admin, sac.address())
}

/// Acceptance 1: protocol manages storage footprint by pruning stale data.
#[test]
fn test_prune_stale_data_after_90_days() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, contract_id, _token_client, token_admin, token_addr) = setup(&env);
    let client = SubStreamContractClient::new(&env, &contract_id);

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);

    client.initialize(&admin);
    client.verify_creator(&admin, &creator);

    token_admin.mint(&subscriber, &10_000);
    env.ledger().set_timestamp(1000);
    client.subscribe(&subscriber, &creator, &token_addr, &10_000, &1, &None);

    // Advance past minimum flow duration and cancel
    env.ledger().set_timestamp(1000 + MINIMUM_DURATION + 1);
    client.cancel(&subscriber, &creator);

    // Advance 90+ days past cancellation
    env.ledger().set_timestamp(1000 + MINIMUM_DURATION + 1 + NINETY_DAYS + 1);

    // Prune should succeed
    client.prune_stale_data(&subscriber, &creator);

    // Tombstone should now exist
    let tombstone = client.get_tombstone(&subscriber, &creator);
    assert!(tombstone.is_some());
}

/// Acceptance 2: tombstones ensure historical integrity while removing heavy data.
#[test]
fn test_tombstone_exists_after_pruning() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, contract_id, _token_client, token_admin, token_addr) = setup(&env);
    let client = SubStreamContractClient::new(&env, &contract_id);

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);

    client.initialize(&admin);
    client.verify_creator(&admin, &creator);

    token_admin.mint(&subscriber, &5_000);
    env.ledger().set_timestamp(500);
    client.subscribe(&subscriber, &creator, &token_addr, &5_000, &1, &None);

    env.ledger().set_timestamp(500 + MINIMUM_DURATION + 1);
    client.cancel(&subscriber, &creator);

    env.ledger().set_timestamp(500 + MINIMUM_DURATION + 1 + NINETY_DAYS + 100);
    client.prune_stale_data(&subscriber, &creator);

    let tombstone = client.get_tombstone(&subscriber, &creator);
    assert!(tombstone.is_some());
    // Tombstone is a 32-byte SHA-256 hash
    assert_eq!(tombstone.unwrap().len(), 32);
}

/// Acceptance 3: the 90-day buffer protects recently canceled subscriptions.
#[test]
#[should_panic(expected = "subscription canceled less than 90 days ago")]
fn test_prune_rejected_before_90_days() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, contract_id, _token_client, token_admin, token_addr) = setup(&env);
    let client = SubStreamContractClient::new(&env, &contract_id);

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);

    client.initialize(&admin);
    client.verify_creator(&admin, &creator);

    token_admin.mint(&subscriber, &5_000);
    env.ledger().set_timestamp(1000);
    client.subscribe(&subscriber, &creator, &token_addr, &5_000, &1, &None);

    env.ledger().set_timestamp(1000 + MINIMUM_DURATION + 1);
    client.cancel(&subscriber, &creator);

    // Only 1 day after cancellation — must be rejected
    env.ledger().set_timestamp(1000 + MINIMUM_DURATION + 1 + 86400);
    client.prune_stale_data(&subscriber, &creator);
}

/// Active subscriptions have no canceled record and cannot be pruned.
#[test]
#[should_panic(expected = "no canceled record found")]
fn test_prune_active_subscription_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, contract_id, _token_client, token_admin, token_addr) = setup(&env);
    let client = SubStreamContractClient::new(&env, &contract_id);

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);

    client.initialize(&admin);
    client.verify_creator(&admin, &creator);

    token_admin.mint(&subscriber, &5_000);
    env.ledger().set_timestamp(1000);
    client.subscribe(&subscriber, &creator, &token_addr, &5_000, &1, &None);

    // Try to prune without canceling first
    env.ledger().set_timestamp(1000 + NINETY_DAYS + 1);
    client.prune_stale_data(&subscriber, &creator);
}
