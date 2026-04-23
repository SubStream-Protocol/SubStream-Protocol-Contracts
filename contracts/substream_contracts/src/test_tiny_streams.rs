#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

fn create_token_contract<'a>(env: &Env, admin: &Address) -> token::Client<'a> {
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    token::Client::new(env, &sac.address())
}

#[test]
fn test_tiny_stream_rounding() {
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&subscriber, &1_000_000_000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    let start = 100u64;
    env.ledger().set_timestamp(start);

    // Suppose we want a stream of 1 unit every 2 seconds (0.5 units/sec).
    // Now we can specify it as 0.5 * PRECISION_MULTIPLIER = 500,000_000
    let rate = 500_000_000; 
    
    // Subscribe with 1000 units
    client.subscribe(&subscriber, &creator, &token.address, &1000, &rate);

    let week = 7 * 24 * 60 * 60;
    env.ledger().set_timestamp(start + week + 100);
    client.collect(&subscriber, &creator);

    // 100 seconds after trial at 0.5 units/sec = 50 units.
    assert_eq!(token.balance(&creator), 50, "Rate 0.5 should stream 50 units in 100 seconds");

    // Now let's try a very low but non-zero rate: 10 units per week.
    // 10 units / (7*24*3600) sec = 10 / 604800 units/sec.
    // Represented in nano: (10 * 10^9) / 604800 = 16534.
    let tiny_rate = 16534; 
    let subscriber2 = Address::generate(&env);
    token_admin.mint(&subscriber2, &1_000_000_000);
    client.subscribe(&subscriber2, &creator, &token.address, &1000, &tiny_rate);
    
    env.ledger().set_timestamp(start + week + week + week + 100); 
    client.collect(&subscriber2, &creator);
    assert_eq!(token.balance(&creator), 50 + 9, "Should be 9 units (9.9997 accrued)");

    // Wait 100 more seconds - the 0.0003 remainder + 100 * 16534 should reach 10 units
    env.ledger().set_timestamp(start + week + week + week + 200);
    client.collect(&subscriber2, &creator);
    assert_eq!(token.balance(&creator), 50 + 10, "Should have accumulated enough dust to reach 10th unit");
}
