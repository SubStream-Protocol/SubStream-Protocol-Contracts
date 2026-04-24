#![cfg(test)]

use super::{
    bulk_import_intent_message, BulkImportItem, Plan, SubStreamContract, SubStreamContractClient,
    SEP12_KYC_ISSUER,
};
use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    token, vec, Address, BytesN, Env, String,
};
use stellar_strkey::{ed25519, Strkey};

const MONTH: u64 = 30 * 24 * 60 * 60;

fn sac_token<'a>(env: &Env, admin: &Address) -> token::Client<'a> {
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    token::Client::new(env, &sac.address())
}

fn account_from_signing_key(env: &Env, sk: &SigningKey) -> (Address, BytesN<32>) {
    let vk = sk.verifying_key();
    let raw: [u8; 32] = vk.to_bytes();
    let pk = ed25519::PublicKey(raw);
    let g = Strkey::PublicKeyEd25519(pk);
    let s = g.to_string();
    let addr = Address::from_str(env, s.as_str());
    (addr, BytesN::from_array(env, &raw))
}

#[test]
fn batch_import_happy_path_merkle_and_billing() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let issuer = Address::from_str(&env, SEP12_KYC_ISSUER);
    let token = sac_token(&env, &admin);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    env.ledger().set_timestamp(10_000);

    client.register_merchant_with_kyc(
        &merchant,
        soroban_sdk::vec![&env, 1u8, 2u8, 3u8],
        &issuer,
    );

    let plan = Plan {
        plan_id: 1,
        name: String::from_str(&env, "Pro"),
        billing_amount: 1_000,
        billing_cycle: MONTH,
        has_trial: false,
        trial_duration: 0,
        is_active: true,
    };
    client.register_plan(&merchant, plan);

    client.set_accepted_token(&merchant, &token.address());

    let sk = SigningKey::from_bytes(&[7u8; 32]);
    let (user, pk_bn) = account_from_signing_key(&env, &sk);

    let msg = bulk_import_intent_message(
        &env,
        &contract_id,
        &merchant,
        &user,
        1u32,
        1u64,
    );
    let sig = sk.sign(msg.as_slice());
    let sig_bn = BytesN::from_array(&env, &sig.to_bytes());

    let item = BulkImportItem {
        user: user.clone(),
        user_public_key: pk_bn,
        plan_id: 1,
        nonce: 1,
        signature: sig_bn,
    };

    client.batch_import_subscriptions(&merchant, &vec![&env, item]);

    assert_eq!(
        client.get_subscription_status(&user, &merchant),
        super::SubscriptionStatus::Active
    );

    let evs = env.events().all();
    assert!(evs.iter().any(|e| {
        matches!(e.topic_0, Some(t) if t == soroban_sdk::symbol_short!("BatchImportExecuted"))
    }));
}

#[test]
#[should_panic]
fn batch_import_rejects_bad_signature() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let issuer = Address::from_str(&env, SEP12_KYC_ISSUER);
    let token = sac_token(&env, &admin);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    env.ledger().set_timestamp(10_000);

    client.register_merchant_with_kyc(
        &merchant,
        soroban_sdk::vec![&env, 9u8],
        &issuer,
    );

    let plan = Plan {
        plan_id: 1,
        name: String::from_str(&env, "Pro"),
        billing_amount: 1_000,
        billing_cycle: MONTH,
        has_trial: false,
        trial_duration: 0,
        is_active: true,
    };
    client.register_plan(&merchant, plan);
    client.set_accepted_token(&merchant, &token.address());

    let sk = SigningKey::from_bytes(&[3u8; 32]);
    let (user, pk_bn) = account_from_signing_key(&env, &sk);

    let item = BulkImportItem {
        user: user.clone(),
        user_public_key: pk_bn,
        plan_id: 1,
        nonce: 1,
        signature: BytesN::from_array(&env, &[9u8; 64]),
    };

    client.batch_import_subscriptions(&merchant, &vec![&env, item]);
}

#[test]
#[should_panic]
fn batch_import_rejects_stale_nonce() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let issuer = Address::from_str(&env, SEP12_KYC_ISSUER);
    let token = sac_token(&env, &admin);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    env.ledger().set_timestamp(10_000);

    client.register_merchant_with_kyc(
        &merchant,
        soroban_sdk::vec![&env, 2u8],
        &issuer,
    );

    let plan = Plan {
        plan_id: 1,
        name: String::from_str(&env, "Pro"),
        billing_amount: 1_000,
        billing_cycle: MONTH,
        has_trial: false,
        trial_duration: 0,
        is_active: true,
    };
    client.register_plan(&merchant, plan);
    client.set_accepted_token(&merchant, &token.address());

    let sk = SigningKey::from_bytes(&[5u8; 32]);
    let (user, pk_bn) = account_from_signing_key(&env, &sk);

    let make_item = |nonce: u64| {
        let msg = bulk_import_intent_message(
            &env,
            &contract_id,
            &merchant,
            &user,
            1u32,
            nonce,
        );
        let sig = sk.sign(msg.as_slice());
        BulkImportItem {
            user: user.clone(),
            user_public_key: pk_bn.clone(),
            plan_id: 1,
            nonce,
            signature: BytesN::from_array(&env, &sig.to_bytes()),
        }
    };

    client.batch_import_subscriptions(&merchant, &vec![&env, make_item(2)]);
    client.batch_import_subscriptions(&merchant, &vec![&env, make_item(1)]);
}

#[test]
#[should_panic]
fn batch_import_rejects_over_50() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let issuer = Address::from_str(&env, SEP12_KYC_ISSUER);
    let tok = sac_token(&env, &admin);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    env.ledger().set_timestamp(10_000);

    client.register_merchant_with_kyc(
        &merchant,
        soroban_sdk::vec![&env, 2u8],
        &issuer,
    );

    let plan = Plan {
        plan_id: 1,
        name: String::from_str(&env, "Pro"),
        billing_amount: 1_000,
        billing_cycle: MONTH,
        has_trial: false,
        trial_duration: 0,
        is_active: true,
    };
    client.register_plan(&merchant, plan);
    client.set_accepted_token(&merchant, &tok.address());

    let sk = SigningKey::from_bytes(&[8u8; 32]);
    let mut v = vec![&env];
    for i in 0u32..51 {
        let mut key_bytes = [8u8; 32];
        key_bytes[0] = (i % 256) as u8;
        key_bytes[1] = (i / 256) as u8;
        let s = SigningKey::from_bytes(&key_bytes);
        let (u, pk_bn) = account_from_signing_key(&env, &s);
        let msg = bulk_import_intent_message(
            &env,
            &contract_id,
            &merchant,
            &u,
            1u32,
            1u64,
        );
        let sig = s.sign(msg.as_slice());
        v.push_back(BulkImportItem {
            user: u,
            user_public_key: pk_bn,
            plan_id: 1,
            nonce: 1,
            signature: BytesN::from_array(&env, &sig.to_bytes()),
        });
    }
    client.batch_import_subscriptions(&merchant, &v);
}
