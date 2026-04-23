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

    let merchant = Address::generate(&env);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

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
