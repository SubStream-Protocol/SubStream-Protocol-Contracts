#![no_main]

use ed25519_dalek::{Signer, SigningKey};
use libfuzzer_sys::fuzz_target;
use soroban_sdk::{testutils::Address as _, token, vec, Address, BytesN, Env, String};
use stellar_strkey::{ed25519, Strkey};

use substream_contracts::{
    BulkImportItem, Plan, SubStreamContract, SubStreamContractClient, SEP12_KYC_ISSUER,
};

const MONTH: u64 = 30 * 24 * 60 * 60;

fn user_from_sk(env: &Env, sk: &SigningKey) -> (Address, BytesN<32>) {
    let raw: [u8; 32] = sk.verifying_key().to_bytes();
    let pk = ed25519::PublicKey(raw);
    let s = Strkey::PublicKeyEd25519(pk).to_string();
    let addr = Address::from_str(env, s.as_str());
    (addr, BytesN::from_array(env, &raw))
}

/// Exercises `batch_import_subscriptions` with **adversarial signature bytes** while the
/// rest of the payload is consistent (valid merchant, plan, binding, nonce). The host should
/// trap on `ed25519_verify` for almost all random inputs; `catch_unwind` keeps the fuzzer alive.
fuzz_target!(|data: &[u8]| {
    if data.len() < 64 {
        return;
    }

    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let issuer = Address::from_str(&env, SEP12_KYC_ISSUER);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let t = token::Client::new(&env, &sac.address());

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);
    env.ledger().set_timestamp(12_000);

    client.register_merchant_with_kyc(
        &merchant,
        soroban_sdk::vec![&env, 0u8],
        &issuer,
    );

    let plan = Plan {
        plan_id: 1,
        name: String::from_str(&env, "Fuzz"),
        billing_amount: 100,
        billing_cycle: MONTH,
        has_trial: false,
        trial_duration: 0,
        is_active: true,
    };
    client.register_plan(&merchant, plan);
    client.set_accepted_token(&merchant, &t.address());

    let mut seed = [11u8; 32];
    for (i, b) in data.iter().enumerate() {
        seed[i % 32] ^= *b;
    }
    let sk = SigningKey::from_bytes(&seed);
    let (user, pk_bn) = user_from_sk(&env, &sk);

    let mut sig = [0u8; 64];
    sig.copy_from_slice(&data[..64]);

    let item = BulkImportItem {
        user,
        user_public_key: pk_bn,
        plan_id: 1,
        nonce: 1,
        signature: BytesN::from_array(&env, &sig),
    };

    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.batch_import_subscriptions(&merchant, &vec![&env, item]);
    }));
});
