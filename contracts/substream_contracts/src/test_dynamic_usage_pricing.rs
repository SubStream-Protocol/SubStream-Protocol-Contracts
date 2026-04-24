#![cfg(test)]

use super::*;
use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    testutils::Events as _,
    token, vec, Address, BytesN, Env, Symbol,
};

const MONTH: u64 = 30 * 24 * 60 * 60;

fn create_token_contract<'a>(env: &Env, admin: &Address) -> token::Client<'a> {
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    token::Client::new(env, &sac.address())
}

fn setup_merchant<'a>(
    env: &Env,
    client: &SubStreamContractClient<'a>,
    admin: &Address,
    merchant: &Address,
    token: &token::Client<'a>,
) {
    client.initialize(admin);
    let kyc_hash = vec![env; 32u8];
    let issuer = Address::from_string(&soroban_sdk::String::from_str(env, crate::SEP12_KYC_ISSUER));
    client.register_merchant_with_kyc(merchant, &kyc_hash, &issuer);
    client.set_accepted_token(merchant, &token.address());
}

fn test_oracle_key() -> SigningKey {
    SigningKey::from_bytes(&[
        0x5a, 0xcc, 0x72, 0x53, 0x29, 0x5d, 0xfc, 0x35, 0x6c, 0x04, 0x62, 0x97, 0x92, 0x5a, 0x36,
        0x9f, 0x3d, 0x27, 0x62, 0xd0, 0x0a, 0xfd, 0xf2, 0x58, 0x3e, 0xcb, 0xe9, 0x21, 0x80, 0xb0,
        0x7c, 0x37,
    ])
}

fn sign_payload(
    env: &Env,
    contract: &Address,
    merchant: &Address,
    subscriber: &Address,
    units: i128,
    usage_ts: u64,
    nonce: u64,
    sk: &SigningKey,
) -> BytesN<64> {
    let msg = dynamic_usage_attestation_message(env, contract, merchant, subscriber, units, usage_ts, nonce);
    let bytes = msg.to_alloc_vec();
    let sig = sk.sign(bytes.as_slice());
    BytesN::from_array(env, &sig.to_bytes())
}

fn approve_for_contract<'a>(
    env: &Env,
    token_admin: &token::StellarAssetClient<'a>,
    subscriber: &Address,
    contract_id: &Address,
    amount: i128,
) {
    let exp = env.ledger().sequence() + 1_000_000u32;
    token_admin.approve(subscriber, contract_id, &amount, &exp);
}

fn base_usage_plan(env: &Env) -> Plan {
    Plan {
        plan_id: 1,
        name: soroban_sdk::String::from_str(env, "Usage API"),
        billing_amount: 1,
        billing_cycle: MONTH,
        has_trial: false,
        trial_duration: 0,
        is_active: true,
    }
}

#[test]
fn test_dynamic_usage_charge_below_cap() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let subscriber = Address::generate(&env);
    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address());
    token_admin.mint(&subscriber, &1_000_000i128);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);
    setup_merchant(&env, &client, &admin, &merchant, &token);

    client.register_plan(&merchant, base_usage_plan(&env));

    let sk = test_oracle_key();
    let vk = sk.verifying_key();
    let mut pk = [0u8; 32];
    pk.copy_from_slice(vk.as_bytes());
    let pk_bn = BytesN::from_array(&env, &pk);

    client.register_dynamic_plan(
        &merchant,
        &1u32,
        &DynamicPlan {
            base_fee: 10,
            per_unit_rate: 3,
        },
    );
    client.set_usage_oracle_signer(&merchant, &pk_bn);

    env.ledger().set_timestamp(10_000);
    client.initialize_subscription(
        &subscriber,
        &merchant,
        &1u32,
        &token.address(),
        &Some(500i128),
    );

    approve_for_contract(&env, &token_admin, &subscriber, &contract_id, 10_000);

    env.ledger().set_timestamp(10_000 + MONTH);

    let units: i128 = 5;
    let usage_ts = 10_000u64 + MONTH - 10;
    let nonce = 1u64;
    let sig = sign_payload(
        &env,
        &contract_id,
        &merchant,
        &subscriber,
        units,
        usage_ts,
        nonce,
        &sk,
    );

    let payload = DynamicUsageOraclePayload {
        subscriber: subscriber.clone(),
        merchant: merchant.clone(),
        units_consumed: units,
        usage_timestamp: usage_ts,
        nonce,
        signature: sig,
    };

    client.execute_subscription_pull(&merchant, &subscriber, &units, &Some(payload));

    let expected = 10i128 + units * 3;
    assert_eq!(token.balance(&merchant), expected);
    assert_eq!(token.balance(&subscriber), 1_000_000 - expected);

    let events = env.events().all();
    assert!(events.iter().any(|e| {
        matches!(e.topic_0, Some(topic) if topic == Symbol::from_str(&env, "DynamicUsageBilled"))
    }));
}

#[test]
fn test_dynamic_usage_charge_capped_at_maximum() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let subscriber = Address::generate(&env);
    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address());
    token_admin.mint(&subscriber, &1_000_000i128);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);
    setup_merchant(&env, &client, &admin, &merchant, &token);

    client.register_plan(&merchant, base_usage_plan(&env));

    let sk = test_oracle_key();
    let vk = sk.verifying_key();
    let mut pk = [0u8; 32];
    pk.copy_from_slice(vk.as_bytes());
    let pk_bn = BytesN::from_array(&env, &pk);

    client.register_dynamic_plan(
        &merchant,
        &1u32,
        &DynamicPlan {
            base_fee: 10,
            per_unit_rate: 5,
        },
    );
    client.set_usage_oracle_signer(&merchant, &pk_bn);

    let cap = 80i128;
    env.ledger().set_timestamp(20_000);
    client.initialize_subscription(
        &subscriber,
        &merchant,
        &1u32,
        &token.address(),
        &Some(cap),
    );

    approve_for_contract(&env, &token_admin, &subscriber, &contract_id, 10_000);

    env.ledger().set_timestamp(20_000 + MONTH);

    let units: i128 = 1_000;
    let usage_ts = 20_000u64 + MONTH - 5;
    let nonce = 42u64;
    let raw = 10i128 + units * 5;
    assert!(raw > cap);

    let sig = sign_payload(
        &env,
        &contract_id,
        &merchant,
        &subscriber,
        units,
        usage_ts,
        nonce,
        &sk,
    );

    let payload = DynamicUsageOraclePayload {
        subscriber: subscriber.clone(),
        merchant: merchant.clone(),
        units_consumed: units,
        usage_timestamp: usage_ts,
        nonce,
        signature: sig,
    };

    client.execute_subscription_pull(&merchant, &subscriber, &units, &Some(payload));

    assert_eq!(token.balance(&merchant), cap);
}

#[test]
#[should_panic(expected = "stale usage timestamp")]
fn test_dynamic_usage_rejects_non_monotonic_timestamp() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let subscriber = Address::generate(&env);
    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address());
    token_admin.mint(&subscriber, &1_000_000i128);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);
    setup_merchant(&env, &client, &admin, &merchant, &token);

    client.register_plan(&merchant, base_usage_plan(&env));

    let sk = test_oracle_key();
    let vk = sk.verifying_key();
    let mut pk = [0u8; 32];
    pk.copy_from_slice(vk.as_bytes());
    let pk_bn = BytesN::from_array(&env, &pk);

    client.register_dynamic_plan(
        &merchant,
        &1u32,
        &DynamicPlan {
            base_fee: 1,
            per_unit_rate: 1,
        },
    );
    client.set_usage_oracle_signer(&merchant, &pk_bn);

    env.ledger().set_timestamp(30_000);
    client.initialize_subscription(
        &subscriber,
        &merchant,
        &1u32,
        &token.address(),
        &Some(500i128),
    );

    approve_for_contract(&env, &token_admin, &subscriber, &contract_id, 10_000);

    env.ledger().set_timestamp(30_000 + MONTH);

    let ts = 30_000u64 + MONTH - 1;
    let p1 = DynamicUsageOraclePayload {
        subscriber: subscriber.clone(),
        merchant: merchant.clone(),
        units_consumed: 2,
        usage_timestamp: ts,
        nonce: 1,
        signature: sign_payload(&env, &contract_id, &merchant, &subscriber, 2, ts, 1, &sk),
    };
    client.execute_subscription_pull(&merchant, &subscriber, &2i128, &Some(p1));

    env.ledger().set_timestamp(30_000 + MONTH + 1);
    let billing_key = DataKey::BillingCycle(subscriber.clone(), merchant.clone());
    let mut billing: BillingCycleInfo = env.storage().persistent().get(&billing_key).unwrap();
    billing.next_billing_date = env.ledger().timestamp();
    env.storage().persistent().set(&billing_key, &billing);

    let p2 = DynamicUsageOraclePayload {
        subscriber: subscriber.clone(),
        merchant: merchant.clone(),
        units_consumed: 3,
        usage_timestamp: ts,
        nonce: 2,
        signature: sign_payload(&env, &contract_id, &merchant, &subscriber, 3, ts, 2, &sk),
    };
    client.execute_subscription_pull(&merchant, &subscriber, &3i128, &Some(p2));
}
