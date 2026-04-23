#![cfg(test)]

use super::*;
use crate::billing_dispute;
use ed25519_dalek::Signer;
use rand::rngs::OsRng;
use soroban_sdk::testutils::Ledger;
use soroban_sdk::{token, vec, Address, BytesN, Env};

const DAY: u64 = 24 * 60 * 60;
const MONTH: u64 = 30 * DAY;

fn sac_token<'a>(env: &Env, admin: &Address) -> token::Client<'a> {
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    token::Client::new(env, &sac.address())
}

fn sign_digest(
    env: &Env,
    sk: &ed25519_dalek::SigningKey,
    dispute_id: u64,
    user_wins: bool,
) -> JurorSignature {
    let digest = billing_dispute::dispute_verdict_digest(env, dispute_id, user_wins);
    let mut msg = [0u8; 32];
    for i in 0..32u32 {
        msg[i as usize] = digest.get(i).unwrap();
    }
    let sig = sk.sign(&msg);
    JurorSignature {
        pubkey: BytesN::from_array(env, &sk.verifying_key().to_bytes()),
        sig: BytesN::from_array(env, &sig.to_bytes()),
    }
}

#[test]
fn overlapping_disputes_escrow_isolation_and_resolution_paths() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let sub_a = Address::generate(&env);
    let sub_b = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token = sac_token(&env, &token_admin);
    let token_sac = token::StellarAssetClient::new(&env, &token.address());
    token_sac.mint(&sub_a, &10_000);
    token_sac.mint(&sub_b, &10_000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    let mut rng = OsRng;
    let sk1 = ed25519_dalek::SigningKey::generate(&mut rng);
    let sk2 = ed25519_dalek::SigningKey::generate(&mut rng);
    let sk3 = ed25519_dalek::SigningKey::generate(&mut rng);
    let jurors = vec![
        &env,
        BytesN::from_array(&env, &sk1.verifying_key().to_bytes()),
        BytesN::from_array(&env, &sk2.verifying_key().to_bytes()),
        BytesN::from_array(&env, &sk3.verifying_key().to_bytes()),
    ];
    client.configure_dispute_jurors(&admin, &jurors);

    let plan = Plan {
        plan_id: 1,
        name: soroban_sdk::String::from_str(&env, "Plan"),
        billing_amount: 100,
        billing_cycle: MONTH,
        has_trial: false,
        trial_duration: 0,
        is_active: true,
    };
    client.register_plan(&merchant, plan);

    env.ledger().set_timestamp(10_000);
    client.initialize_subscription(&sub_a, &merchant, &1, &token.address());
    client.initialize_subscription(&sub_b, &merchant, &1, &token.address());

    token.approve(&sub_a, &contract_id, &5000, &1_000_000u32);
    token.approve(&sub_b, &contract_id, &5000, &1_000_000u32);

    env.ledger().set_timestamp(10_000 + MONTH);
    client.execute_subscription_pull(&merchant, &sub_a);
    client.execute_subscription_pull(&merchant, &sub_b);

    client.raise_dispute(&sub_a, &merchant, &5);
    client.raise_dispute(&sub_b, &merchant, &7);

    let d1 = client.get_active_dispute_id(&sub_a, &merchant).unwrap();
    let d2 = client.get_active_dispute_id(&sub_b, &merchant).unwrap();
    assert_ne!(d1, d2);

    let r1 = client.get_dispute_record(&d1).unwrap();
    let r2 = client.get_dispute_record(&d2).unwrap();
    assert_eq!(r1.disputed_amount, 100);
    assert_eq!(r1.bond_amount, 5);
    assert_eq!(r2.disputed_amount, 100);
    assert_eq!(r2.bond_amount, 7);

    let sigs_user = vec![
        &env,
        sign_digest(&env, &sk1, d1, true),
        sign_digest(&env, &sk2, d1, true),
        sign_digest(&env, &sk3, d1, true),
    ];
    client.resolve_dispute_for_user(&sub_a, &merchant, &d1, &sigs_user);

    let sigs_merchant = vec![
        &env,
        sign_digest(&env, &sk1, d2, false),
        sign_digest(&env, &sk2, d2, false),
        sign_digest(&env, &sk3, d2, false),
    ];
    client.resolve_dispute_for_merchant(&sub_b, &merchant, &d2, &sigs_merchant);

    assert_eq!(token.balance(&sub_a), 10_000);
    assert_eq!(token.balance(&sub_b), 9_893);
    assert_eq!(token.balance(&merchant), 107);
}
