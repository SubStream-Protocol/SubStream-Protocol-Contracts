#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, token, vec, Address, Env};

const DAY: u64 = 24 * 60 * 60;
const WEEK: u64 = 7 * DAY;

fn create_token_contract<'a>(env: &Env, admin: &Address) -> token::Client<'a> {
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    token::Client::new(env, &sac.address())
}

fn register_verified_merchant(client: &SubStreamContractClient, env: &Env, merchant: &Address) {
    let issuer = Address::from_string(&soroban_sdk::String::from_str(env, SEP12_KYC_ISSUER));
    client.register_merchant_with_kyc(merchant, &vec![env, 1u8, 2u8, 3u8], &issuer);
}

#[test]
fn wrapped_subscription_access_tracks_token_owner() {
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let new_owner = Address::generate(&env);
    let merchant = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&subscriber, &10_000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    env.ledger().set_timestamp(100);
    register_verified_merchant(&client, &env, &merchant);
    client.subscribe(
        &subscriber,
        &merchant,
        &token.address,
        &1_000,
        &1_000_000_000,
        &None,
    );

    let token_id = client.enable_subscription_transferability(&subscriber, &merchant);
    assert!(client.check_access(&token_id, &subscriber));
    assert!(!client.check_access(&token_id, &new_owner));

    client.transfer_subscription_token(&token_id, &new_owner);
    assert!(!client.check_access(&token_id, &subscriber));
    assert!(client.check_access(&token_id, &new_owner));
    assert!(client.is_subscribed(&new_owner, &merchant));
}

#[test]
fn transfer_settles_accrued_amount_before_owner_switch() {
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let new_owner = Address::generate(&env);
    let merchant = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&subscriber, &10_000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    let start = 100u64;
    env.ledger().set_timestamp(start);
    register_verified_merchant(&client, &env, &merchant);
    client.subscribe(
        &subscriber,
        &merchant,
        &token.address,
        &1_000,
        &1_000_000_000,
        &None,
    );
    let token_id = client.enable_subscription_transferability(&subscriber, &merchant);

    env.ledger().set_timestamp(start + WEEK + 5);
    client.transfer_subscription_token(&token_id, &new_owner);
    assert_eq!(token.balance(&merchant), 5);

    env.ledger().set_timestamp(start + WEEK + 8);
    client.execute_pull(&token_id);
    assert_eq!(token.balance(&merchant), 8);
}
