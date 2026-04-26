//! Tests for Issue #131: check_allowance_health Pre-flight Check
#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

fn setup(env: &Env) -> (Address, Address, token::Client, token::StellarAssetClient, Address) {
    let admin = Address::generate(env);
    let contract_id = env.register(SubStreamContract, ());
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token_client = token::Client::new(env, &sac.address());
    let token_admin = token::StellarAssetClient::new(env, &sac.address());
    (admin, contract_id, token_client, token_admin, sac.address())
}

/// Acceptance 1: merchants can proactively detect impending payment failures.
#[test]
fn test_check_allowance_health_healthy() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, contract_id, token_client, token_admin, token_addr) = setup(&env);
    let client = SubStreamContractClient::new(&env, &contract_id);

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);

    client.initialize(&admin);
    client.verify_creator(&admin, &creator);

    // Fund subscriber with plenty of tokens
    token_admin.mint(&subscriber, &10_000);
    client.subscribe(&subscriber, &creator, &token_addr, &10_000, &1, &None);

    let health = client.check_allowance_health(&subscriber, &creator, &token_addr);
    assert_eq!(health, AllowanceHealth::Healthy);
}

/// Acceptance 2: gas waste is minimized by detecting doomed transactions early.
#[test]
fn test_check_allowance_health_insufficient_funds() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, contract_id, token_client, token_admin, token_addr) = setup(&env);
    let client = SubStreamContractClient::new(&env, &contract_id);

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);

    client.initialize(&admin);
    client.verify_creator(&admin, &creator);

    // Subscribe with a very small balance relative to rate
    token_admin.mint(&subscriber, &1);
    client.subscribe(&subscriber, &creator, &token_addr, &1, &1_000_000, &None);

    // Advance time past trial so balance is effectively exhausted
    env.ledger().set_timestamp(8 * 24 * 60 * 60);

    let health = client.check_allowance_health(&subscriber, &creator, &token_addr);
    // Balance (1 token) < rate (1_000_000 per second) → InsufficientFunds
    assert_eq!(health, AllowanceHealth::InsufficientFunds);
}

/// Acceptance 3: user privacy is maintained — abstract status, not exact balance.
#[test]
fn test_check_allowance_health_revoked_no_subscription() {
    let env = Env::default();
    env.mock_all_auths();
    let (admin, contract_id, _token_client, _token_admin, token_addr) = setup(&env);
    let client = SubStreamContractClient::new(&env, &contract_id);

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);

    // No subscription exists → AllowanceRevoked
    let health = client.check_allowance_health(&subscriber, &creator, &token_addr);
    assert_eq!(health, AllowanceHealth::AllowanceRevoked);
}
