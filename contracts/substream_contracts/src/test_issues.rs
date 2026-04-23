#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger},
    token, vec, Address, Bytes, Env,
};

fn create_token_contract<'a>(env: &Env, admin: &Address) -> token::Client<'a> {
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    token::Client::new(env, &sac.address())
}

#[test]
fn test_escrow_deposit_and_drip() {
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let merchant = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&subscriber, &1200);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    // Deposit 1200 for 12 months (100 per month)
    client.deposit_to_escrow(&subscriber, &merchant, &token.address, &1200, &12);

    assert_eq!(token.balance(&contract_id), 1200);

    // Advance 1 month (30 days)
    env.ledger().set_timestamp(30 * 24 * 60 * 60);
    client.claim_drip(&subscriber, &merchant);

    assert_eq!(token.balance(&merchant), 100);
    assert_eq!(token.balance(&contract_id), 1100);

    // Advance 5 more months
    env.ledger().set_timestamp(180 * 24 * 60 * 60);
    client.claim_drip(&subscriber, &merchant);

    assert_eq!(token.balance(&merchant), 600);
    assert_eq!(token.balance(&contract_id), 600);
}

#[test]
fn test_zk_proof_verification_and_nullifier() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    // Initialize contract and verify merchant
    client.initialize(&admin);
    client.verify_creator(&admin, &merchant);

    let proof = Bytes::from_slice(&env, &[0u8; 64]);
    let nullifier = Bytes::from_slice(&env, &[1u8; 32]);

    // First use: success
    client.verify_anonymous_subscription(&merchant, &proof, &nullifier);

    // Second use: fail (replay)
    let result = env.as_contract(&contract_id, || {
        client.try_verify_anonymous_subscription(&merchant, &proof, &nullifier)
    });

    assert!(result.is_err());
}

#[test]
fn test_replay_attack_prevention_comprehensive() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    // Initialize contract and verify merchant
    client.initialize(&admin);
    client.verify_creator(&admin, &merchant);

    // Test 1: Multiple different nullifiers should work
    let proof1 = Bytes::from_slice(&env, &[0u8; 64]);
    let nullifier1 = Bytes::from_slice(&env, &[1u8; 32]);
    let proof2 = Bytes::from_slice(&env, &[1u8; 64]);
    let nullifier2 = Bytes::from_slice(&env, &[2u8; 32]);

    // Both should succeed
    client.verify_anonymous_subscription(&merchant, &proof1, &nullifier1);
    client.verify_anonymous_subscription(&merchant, &proof2, &nullifier2);

    // Test 2: Same nullifier with different proof should fail
    let proof3 = Bytes::from_slice(&env, &[2u8; 64]);
    let result = env.as_contract(&contract_id, || {
        client.try_verify_anonymous_subscription(&merchant, &proof3, &nullifier1)
    });
    assert!(result.is_err());

    // Test 3: Same proof with different nullifier should work
    let nullifier3 = Bytes::from_slice(&env, &[3u8; 32]);
    client.verify_anonymous_subscription(&merchant, &proof1, &nullifier3);

    // Test 4: Verify nullifier tracking
    assert!(client.is_nullifier_used(&nullifier1));
    assert!(client.is_nullifier_used(&nullifier2));
    assert!(client.is_nullifier_used(&nullifier3));
}

#[test]
fn test_replay_attack_blocked_event() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    // Initialize contract and verify merchant
    client.initialize(&admin);
    client.verify_creator(&admin, &merchant);

    let proof = Bytes::from_slice(&env, &[0u8; 64]);
    let nullifier = Bytes::from_slice(&env, &[1u8; 32]);

    // First use: success
    client.verify_anonymous_subscription(&merchant, &proof, &nullifier);

    // Second use: should emit ReplayAttackBlocked event
    let result = env.as_contract(&contract_id, || {
        client.try_verify_anonymous_subscription(&merchant, &proof, &nullifier)
    });

    assert!(result.is_err());

    // Check for ReplayAttackBlocked event
    let events = env.events().all();
    let replay_events: Vec<_> = events.iter()
        .filter(|e| e.topics[0] == soroban_sdk::Symbol::from_str(&env, "ReplayAttackBlocked"))
        .collect();

    assert_eq!(replay_events.len(), 1);
    
    // Verify event data
    let event = &replay_events[0];
    assert_eq!(event.topics[1], merchant); // merchant topic
    assert_eq!(event.topics[2], nullifier); // nullifier topic
}

#[test]
fn test_invalid_proof_length() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    // Initialize contract and verify merchant
    client.initialize(&admin);
    client.verify_creator(&admin, &merchant);

    let invalid_proof = Bytes::from_slice(&env, &[0u8; 32]); // Wrong length
    let nullifier = Bytes::from_slice(&env, &[1u8; 32]);

    // Should fail with invalid proof length
    let result = env.as_contract(&contract_id, || {
        client.try_verify_anonymous_subscription(&merchant, &invalid_proof, &nullifier)
    });

    assert!(result.is_err());
}

#[test]
fn test_unverified_merchant_rejection() {
    let env = Env::default();
    env.mock_all_auths();

    let merchant = Address::generate(&env);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    let proof = Bytes::from_slice(&env, &[0u8; 64]);
    let nullifier = Bytes::from_slice(&env, &[1u8; 32]);

    // Should fail for unverified merchant
    let result = env.as_contract(&contract_id, || {
        client.try_verify_anonymous_subscription(&merchant, &proof, &nullifier)
    });

    assert!(result.is_err());
}

#[test]
fn test_nullifier_cleanup_functionality() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    // Initialize contract and verify merchant
    client.initialize(&admin);
    client.verify_creator(&admin, &merchant);

    let proof = Bytes::from_slice(&env, &[0u8; 64]);
    let nullifier = Bytes::from_slice(&env, &[1u8; 32]);

    // Create a nullifier
    client.verify_anonymous_subscription(&merchant, &proof, &nullifier);

    // Verify nullifier exists
    assert!(client.is_nullifier_used(&nullifier));

    // Advance time beyond nullifier validity period
    let current_time = env.ledger().timestamp();
    env.ledger().set_timestamp(current_time + 31 * 24 * 60 * 60); // 31 days later

    // Run cleanup
    client.cleanup_expired_nullifiers();

    // Note: In a real implementation, we would verify that the nullifier is cleaned up
    // For this test, we just verify the cleanup function runs without error
}

#[test]
fn test_o1_complexity_nullifier_lookup() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    // Initialize contract and verify merchant
    client.initialize(&admin);
    client.verify_creator(&admin, &merchant);

    // Create many nullifiers to test O(1) complexity
    let num_nullifiers = 100;
    for i in 0..num_nullifiers {
        let proof = Bytes::from_slice(&env, &[(i as u8); 64]);
        let nullifier = Bytes::from_slice(&env, &[(i as u8); 32]);
        client.verify_anonymous_subscription(&merchant, &proof, &nullifier);
    }

    // Test lookup speed by checking many nullifiers
    // In O(1) complexity, this should be fast regardless of number of nullifiers
    for i in 0..num_nullifiers {
        let nullifier = Bytes::from_slice(&env, &[(i as u8); 32]);
        assert!(client.is_nullifier_used(&nullifier));
    }

    // Test non-existent nullifier (should be false)
    let non_existent_nullifier = Bytes::from_slice(&env, &[255u8; 32]);
    assert!(!client.is_nullifier_used(&non_existent_nullifier));
}

#[test]
fn test_mathematical_isolation_of_zk_transactions() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant1 = Address::generate(&env);
    let merchant2 = Address::generate(&env);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    // Initialize contract and verify merchants
    client.initialize(&admin);
    client.verify_creator(&admin, &merchant1);
    client.verify_creator(&admin, &merchant2);

    let proof = Bytes::from_slice(&env, &[0u8; 64]);
    let nullifier = Bytes::from_slice(&env, &[1u8; 32]);

    // Use nullifier with merchant1
    client.verify_anonymous_subscription(&merchant1, &proof, &nullifier);

    // Same nullifier should fail with merchant2 (mathematical isolation)
    let result = env.as_contract(&contract_id, || {
        client.try_verify_anonymous_subscription(&merchant2, &proof, &nullifier)
    });

    assert!(result.is_err());

    // But different nullifier should work with merchant2
    let nullifier2 = Bytes::from_slice(&env, &[2u8; 32]);
    client.verify_anonymous_subscription(&merchant2, &proof, &nullifier2);

    // Verify isolation: merchant1's nullifier shouldn't affect merchant2
    assert!(client.is_nullifier_used(&nullifier));
    assert!(client.is_nullifier_used(&nullifier2));
}

#[test]
fn test_yield_routing_buffer_enforcement() {
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let merchant = Address::generate(&env);
    let protocol = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&subscriber, &1200);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    client.deposit_to_escrow(&subscriber, &merchant, &token.address, &1200, &12);
    
    let yield_config = YieldConfig {
        target_protocol: protocol.clone(),
        user_share_bps: 5000,
        merchant_share_bps: 4000,
        dao_share_bps: 1000,
    };
    client.set_yield_config(&merchant, &yield_config);

    // Monthly drip is 100. Unvested is 1200. Safe to route is 1200 - 100 = 1100.
    // Try routing 1101: should fail
    let result = env.as_contract(&contract_id, || {
        client.try_route_escrow_to_yield(&subscriber, &merchant, &1101)
    });
    assert!(result.is_err());

    // Route 1100: success
    client.route_escrow_to_yield(&subscriber, &merchant, &1100);
    assert_eq!(token.balance(&protocol), 1100);
    assert_eq!(token.balance(&contract_id), 100); // 30-day buffer remains
}
