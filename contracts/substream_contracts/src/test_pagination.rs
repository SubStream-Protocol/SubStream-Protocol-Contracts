//! Tests for Issue #129: Cursor-Based Pagination to User Queries
#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

fn setup_env() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(SubStreamContract, ());
    (env, admin, contract_id)
}

fn mint_and_subscribe(
    env: &Env,
    client: &SubStreamContractClient,
    subscriber: &Address,
    creator: &Address,
    token_client: &token::Client,
    token_admin: &token::StellarAssetClient,
    amount: i128,
    rate: i128,
) {
    token_admin.mint(subscriber, &amount);
    client.subscribe(subscriber, creator, &token_client.address, &amount, &rate, &None);
}

/// Acceptance 1: protocol scales without breaking under response size limits.
#[test]
fn test_pagination_empty_returns_empty_page() {
    let (env, _admin, contract_id) = setup_env();
    let client = SubStreamContractClient::new(&env, &contract_id);
    let subscriber = Address::generate(&env);

    let page = client.get_subscriptions_paginated(&subscriber, &0, &10);
    assert_eq!(page.items.len(), 0);
    assert_eq!(page.next_cursor, 0);
    assert!(!page.has_more);
}

/// Acceptance 2: clients can page through datasets without performance degradation.
#[test]
fn test_pagination_pages_through_20_subscriptions() {
    let (env, admin, contract_id) = setup_env();
    let client = SubStreamContractClient::new(&env, &contract_id);

    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token_client = token::Client::new(&env, &sac.address());
    let token_admin = token::StellarAssetClient::new(&env, &sac.address());

    let subscriber = Address::generate(&env);

    // Create 20 creators and subscribe to each
    for _ in 0..20u32 {
        let creator = Address::generate(&env);
        // Register creator as verified merchant
        client.initialize(&admin);
        client.verify_creator(&admin, &creator);
        mint_and_subscribe(&env, &client, &subscriber, &creator, &token_client, &token_admin, 1000, 1);
    }

    // Page through with page size 5 — should get all 20 records across 4 pages
    let mut collected = 0u32;
    let mut cursor = 0u32;
    loop {
        let page = client.get_subscriptions_paginated(&subscriber, &cursor, &5);
        collected += page.items.len();
        cursor = page.next_cursor;
        if !page.has_more {
            break;
        }
    }
    assert_eq!(collected, 20);
}

/// Acceptance 3: cursor logic is deterministic and immune to out-of-bounds panics.
#[test]
fn test_pagination_out_of_bounds_cursor_returns_empty() {
    let (env, admin, contract_id) = setup_env();
    let client = SubStreamContractClient::new(&env, &contract_id);

    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token_client = token::Client::new(&env, &sac.address());
    let token_admin = token::StellarAssetClient::new(&env, &sac.address());

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);
    client.initialize(&admin);
    client.verify_creator(&admin, &creator);
    mint_and_subscribe(&env, &client, &subscriber, &creator, &token_client, &token_admin, 1000, 1);

    // Cursor beyond the end must not panic
    let page = client.get_subscriptions_paginated(&subscriber, &999, &5);
    assert_eq!(page.items.len(), 0);
    assert!(!page.has_more);
}

/// Zero limit returns empty page without panicking.
#[test]
fn test_pagination_zero_limit_returns_empty() {
    let (env, _admin, contract_id) = setup_env();
    let client = SubStreamContractClient::new(&env, &contract_id);
    let subscriber = Address::generate(&env);

    let page = client.get_subscriptions_paginated(&subscriber, &0, &0);
    assert_eq!(page.items.len(), 0);
    assert!(!page.has_more);
}

/// get_active_subscriptions returns all subscriptions via the index.
#[test]
fn test_get_active_subscriptions_uses_index() {
    let (env, admin, contract_id) = setup_env();
    let client = SubStreamContractClient::new(&env, &contract_id);

    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token_client = token::Client::new(&env, &sac.address());
    let token_admin = token::StellarAssetClient::new(&env, &sac.address());

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);
    client.initialize(&admin);
    client.verify_creator(&admin, &creator);
    mint_and_subscribe(&env, &client, &subscriber, &creator, &token_client, &token_admin, 1000, 1);

    let subs = client.get_active_subscriptions(&subscriber);
    assert_eq!(subs.len(), 1);
}
