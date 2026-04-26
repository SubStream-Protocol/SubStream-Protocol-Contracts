/// Issue #138: Stress Test – 500 Sequential Pulls in One Ledger
///
/// Soroban is single-threaded; "concurrent" pulls are modelled as 500 independent
/// subscriber→merchant streams all collected in the same ledger timestamp.
///
/// Acceptance 1: High-traffic billing days do not trigger panics for merchants.
/// Acceptance 2: Storage optimisations prove efficiency under extreme transactional load.
/// Acceptance 3: All 500 pulls succeed and balances are consistent.
#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

const WEEK: u64 = 7 * 24 * 60 * 60;
const RATE: i128 = PRECISION_MULTIPLIER; // 1 token/sec
const DEPOSIT: i128 = 1_000_000 * PRECISION_MULTIPLIER; // large buffer

fn setup_merchant(env: &Env) -> (SubStreamContractClient, Address, token::Client, token::StellarAssetClient) {
    let admin = Address::generate(env);
    let merchant = Address::generate(env);
    let token_admin = Address::generate(env);

    let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token = token::Client::new(env, &sac.address());
    let token_sa = token::StellarAssetClient::new(env, &sac.address());

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(env, &contract_id);
    client.initialize(&admin);

    env.as_contract(&contract_id, || {
        env.storage().persistent().set(
            &DataKey::MerchantRegistry(merchant.clone()),
            &MerchantStatus {
                is_verified: true,
                is_blacklisted: false,
                verification_method: VerificationMethod::DAOApproval,
                registered_at: 0,
                last_verified: 0,
                dao_approved: true,
            },
        );
    });

    (client, merchant, token, token_sa)
}

#[test]
fn test_500_sequential_pulls_no_panic() {
    const N: usize = 500;

    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let (client, merchant, token, token_sa) = setup_merchant(&env);

    // Subscribe N unique subscribers
    let mut subscribers: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    for _ in 0..N {
        let sub = Address::generate(&env);
        token_sa.mint(&sub, &DEPOSIT);
        client.subscribe(&sub, &merchant, &token.address, &DEPOSIT, &RATE, &None);
        subscribers.push_back(sub);
    }

    // Advance past the 7-day trial so charges accrue
    let collect_time = 1_000_000 + WEEK + 3600; // 1 hour into paid period
    env.ledger().set_timestamp(collect_time);

    // Collect from all 500 subscribers in the same ledger timestamp
    let mut successful_pulls: usize = 0;
    for i in 0..N {
        let sub = subscribers.get(i as u32).unwrap();
        client.collect(&sub, &merchant);
        successful_pulls += 1;
    }

    assert_eq!(successful_pulls, N, "all 500 pulls must succeed without panic");

    // Verify merchant received tokens from all subscribers
    let merchant_balance = token.balance(&merchant);
    assert!(
        merchant_balance > 0,
        "merchant must have received tokens from 500 pulls; got {}",
        merchant_balance
    );
}

#[test]
fn test_500_pulls_balance_consistency() {
    const N: usize = 500;

    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let (client, merchant, token, token_sa) = setup_merchant(&env);

    let mut subscribers: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    for _ in 0..N {
        let sub = Address::generate(&env);
        token_sa.mint(&sub, &DEPOSIT);
        client.subscribe(&sub, &merchant, &token.address, &DEPOSIT, &RATE, &None);
        subscribers.push_back(sub);
    }

    let collect_time = 1_000_000 + WEEK + 7200; // 2 hours into paid period
    env.ledger().set_timestamp(collect_time);

    let contract_id = env.register(SubStreamContract, ());
    let contract_balance_before = token.balance(&contract_id);

    for i in 0..N {
        let sub = subscribers.get(i as u32).unwrap();
        client.collect(&sub, &merchant);
    }

    let contract_balance_after = token.balance(&contract_id);
    let merchant_balance = token.balance(&merchant);

    // Tokens must be conserved: contract lost what merchant gained
    assert_eq!(
        contract_balance_before - contract_balance_after,
        merchant_balance,
        "token conservation: contract outflow must equal merchant inflow"
    );
}
