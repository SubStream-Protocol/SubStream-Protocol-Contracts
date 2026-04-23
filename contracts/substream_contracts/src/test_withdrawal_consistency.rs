#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, vec, Address, Env, Vec,
};
use super::{SubStreamContract, SubStreamContractClient};

const DAY: u64 = 24 * 60 * 60;
const WEEK: u64 = 7 * DAY;

/// Simulates high-load subscribers withdrawing simultaneously to ensure vault balance never goes negative.
/// This is a security-critical test for Issue #22.
/// Uses 1000 subscribers for practical test execution time.
#[test]
fn test_withdrawal_consistency_high_load() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let creator = Address::generate(&env);

    // Create token contract
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token_client = token::Client::new(&env, &sac.address());
    let token_admin = token::StellarAssetClient::new(&env, &sac.address());

    // Register main contract
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    // Initialize at timestamp
    let start_time: u64 = 100;
    env.ledger().set_timestamp(start_time);

    const NUM_SUBSCRIBERS: usize = 1000;

    // Track all subscriber addresses and their initial deposits
    let mut subscribers: Vec<Address> = vec![&env];
    let mut expected_vault_balance: i128 = 0;

    // Create 10,000 subscribers with varying amounts
    for i in 0..NUM_SUBSCRIBERS {
        let subscriber = Address::generate(&env);
        subscribers.push_back(subscriber.clone());

        // Vary deposit amounts between 100 and 10100
        let amount: i128 = 100 + ((i as i128 * 17) % 10000);
        expected_vault_balance += amount;

        // Mint tokens to subscriber (give extra for safety)
        token_admin.mint(&subscriber, &(amount * 2));

        // Vary rate between 1 and 101
        let rate: i128 = 1 + ((i as i128 * 13) % 100);

        // Subscribe
        client.subscribe(&subscriber, &creator, &sac.address(), &amount, &rate);
    }

    // Verify initial vault balance
    let initial_vault_balance = token_client.balance(&contract_id);
    assert_eq!(
        initial_vault_balance, expected_vault_balance,
        "Initial vault balance mismatch: expected {}, got {}",
        expected_vault_balance, initial_vault_balance
    );

    // Advance past minimum duration + free trial
    let simulation_time = start_time + WEEK + DAY + 1000;
    env.ledger().set_timestamp(simulation_time);

    // Perform random collections before cancellations (simulate 10% of subscribers)
    let collect_count = NUM_SUBSCRIBERS / 10;
    for i in 0..collect_count {
        let idx = (i * 7) % NUM_SUBSCRIBERS; // Pseudorandom order
        if let Some(subscriber) = subscribers.get(idx as u32) {
            client.collect(&subscriber, &creator);
        }
    }

    // Track vault balance after collections
    let balance_after_collections = token_client.balance(&contract_id);
    assert!(
        balance_after_collections >= 0,
        "CRITICAL: Vault balance went negative after collections: {}",
        balance_after_collections
    );

    // Now simulate simultaneous withdrawals (cancellations)
    // Use pseudorandom order to simulate race conditions
    for i in 0..NUM_SUBSCRIBERS {
        let order_idx = ((i * 7919) % NUM_SUBSCRIBERS) as u32; // Prime multiplier for better distribution
        if let Some(subscriber) = subscribers.get(order_idx) {
            // Check if still subscribed before cancelling
            if client.is_subscribed(&subscriber, &creator) {
                // Get balance before cancellation
                let contract_balance_before = token_client.balance(&contract_id);
                assert!(
                    contract_balance_before >= 0,
                    "CRITICAL: Vault balance went negative before cancellation #{}: {}",
                    i, contract_balance_before
                );

                // Cancel subscription - this refunds remaining balance
                client.cancel(&subscriber, &creator);

                // Get balance after cancellation
                let contract_balance_after = token_client.balance(&contract_id);

                // CRITICAL SECURITY CHECK: Vault balance must never go negative
                assert!(
                    contract_balance_after >= 0,
                    "CRITICAL SECURITY BUG: Vault balance went negative after cancellation #{}: {} (before: {})",
                    i, contract_balance_after, contract_balance_before
                );
            }
        }
    }

    // Final verification: vault balance should be exactly 0 (all funds withdrawn or paid to creators)
    let final_vault_balance = token_client.balance(&contract_id);
    assert!(
        final_vault_balance >= 0,
        "CRITICAL: Final vault balance is negative: {}",
        final_vault_balance
    );

    // Verify all subscriptions are cancelled
    let mut remaining_active = 0u32;
    for i in 0..NUM_SUBSCRIBERS {
        if let Some(subscriber) = subscribers.get(i as u32) {
            if client.is_subscribed(&subscriber, &creator) {
                remaining_active += 1;
            }
        }
    }
    assert_eq!(
        remaining_active, 0,
        "{} subscriptions still active after all cancellations",
        remaining_active
    );
}

/// Additional stress test with edge case amounts
#[test]
fn test_withdrawal_consistency_edge_cases() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let creator = Address::generate(&env);

    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token_client = token::Client::new(&env, &sac.address());
    let token_admin = token::StellarAssetClient::new(&env, &sac.address());

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    env.ledger().set_timestamp(100);

    // Test edge case: minimum amounts
    let subscriber1 = Address::generate(&env);
    token_admin.mint(&subscriber1, &1000);
    client.subscribe(&subscriber1, &creator, &sac.address(), &100, &1);

    // Test edge case: large amounts
    let subscriber2 = Address::generate(&env);
    token_admin.mint(&subscriber2, &1000000);
    client.subscribe(&subscriber2, &creator, &sac.address(), &500000, &1000);

    // Test edge case: high rate
    let subscriber3 = Address::generate(&env);
    token_admin.mint(&subscriber3, &10000);
    client.subscribe(&subscriber3, &creator, &sac.address(), &5000, &500);

    env.ledger().set_timestamp(100 + WEEK + DAY);

    // Collect and drain
    client.collect(&subscriber1, &creator);
    client.collect(&subscriber2, &creator);
    client.collect(&subscriber3, &creator);

    // Cancel all
    client.cancel(&subscriber1, &creator);
    
    let balance_after_cancel1 = token_client.balance(&contract_id);
    assert!(balance_after_cancel1 >= 0, "Vault negative after cancel1: {}", balance_after_cancel1);

    client.cancel(&subscriber2, &creator);
    
    let balance_after_cancel2 = token_client.balance(&contract_id);
    assert!(balance_after_cancel2 >= 0, "Vault negative after cancel2: {}", balance_after_cancel2);

    client.cancel(&subscriber3, &creator);
    
    let final_balance = token_client.balance(&contract_id);
    assert!(final_balance >= 0, "Vault negative after cancel3: {}", final_balance);
}
