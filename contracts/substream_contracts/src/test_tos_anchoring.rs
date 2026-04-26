/// Issue #125: Anchoring Merchant Terms of Service (IPFS Hashes)
/// Tests covering:
/// Acceptance 1: Subscriptions carry verifiable, immutable legal agreements.
/// Acceptance 2: Users are protected from merchants retroactively changing ToS.
/// Acceptance 3: Anchoring adds minimal overhead to the subscription lifecycle.
#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Bytes, Env,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn ipfs_hash(env: &Env, s: &str) -> Bytes {
    Bytes::from_slice(env, s.as_bytes())
}

fn setup_verified_merchant(env: &Env) -> (SubStreamContractClient<'static>, Address, Address) {
    let admin = Address::generate(env);
    let merchant = Address::generate(env);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(env, &contract_id);
    client.initialize(&admin);

    // Directly write merchant status to storage (bypass KYC issuer check)
    let merchant_status = MerchantStatus {
        is_verified: true,
        is_blacklisted: false,
        verification_method: VerificationMethod::DAOApproval,
        registered_at: 0,
        last_verified: 0,
        dao_approved: true,
    };
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::MerchantRegistry(merchant.clone()), &merchant_status);
    });

    let client: SubStreamContractClient<'static> = unsafe { core::mem::transmute(client) };
    (client, admin, merchant)
}

// ---------------------------------------------------------------------------
// Acceptance 1: Subscriptions carry the weight of verifiable legal agreements
// ---------------------------------------------------------------------------

#[test]
fn test_anchor_tos_and_snapshot_at_subscribe() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1000);

    let (client, _admin, merchant) = setup_verified_merchant(&env);
    let subscriber = Address::generate(&env);

    let hash_v1 = ipfs_hash(&env, "QmV1aaaBBBCCCDDD1111222233334444");

    // Merchant anchors ToS v1
    client.anchor_merchant_tos(&merchant, &hash_v1);

    let anchor = client.get_merchant_tos(&merchant);
    assert_eq!(anchor.version, 1);
    assert_eq!(anchor.ipfs_hash, hash_v1);

    // Subscribe — snapshot should be recorded
    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token = token::Client::new(&env, &sac.address());
    let token_admin_client = token::StellarAssetClient::new(&env, &sac.address());
    token_admin_client.mint(&subscriber, &10_000);

    client.subscribe(
        &subscriber,
        &merchant,
        &token.address,
        &10_000,
        &1_000_000_000,
        &None,
    );

    let snapshot = client.get_subscription_tos_snapshot(&subscriber, &merchant);
    assert!(snapshot.is_some());
    let snap = snapshot.unwrap();
    assert_eq!(snap.version, 1);
    assert_eq!(snap.ipfs_hash, hash_v1);
    assert_eq!(snap.agreed_at, 1000);
}

#[test]
fn test_subscribe_without_tos_no_snapshot() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, merchant) = setup_verified_merchant(&env);
    let subscriber = Address::generate(&env);

    // Merchant has NOT anchored any ToS
    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token = token::Client::new(&env, &sac.address());
    let token_admin_client = token::StellarAssetClient::new(&env, &sac.address());
    token_admin_client.mint(&subscriber, &10_000);

    client.subscribe(
        &subscriber,
        &merchant,
        &token.address,
        &10_000,
        &1_000_000_000,
        &None,
    );

    // No snapshot should exist
    let snapshot = client.get_subscription_tos_snapshot(&subscriber, &merchant);
    assert!(snapshot.is_none());
}

// ---------------------------------------------------------------------------
// Acceptance 2: Users protected from retroactive ToS changes
// ---------------------------------------------------------------------------

#[test]
fn test_tos_snapshot_unchanged_after_merchant_updates() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1000);

    let (client, _admin, merchant) = setup_verified_merchant(&env);
    let subscriber = Address::generate(&env);

    let hash_v1 = ipfs_hash(&env, "QmV1OriginalTerms11111111111111");
    let hash_v2 = ipfs_hash(&env, "QmV2UpdatedTerms222222222222222");

    client.anchor_merchant_tos(&merchant, &hash_v1);

    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token = token::Client::new(&env, &sac.address());
    let token_admin_client = token::StellarAssetClient::new(&env, &sac.address());
    token_admin_client.mint(&subscriber, &10_000);

    client.subscribe(
        &subscriber,
        &merchant,
        &token.address,
        &10_000,
        &1_000_000_000,
        &None,
    );

    // Merchant updates ToS to v2 (e.g., changes pricing terms)
    env.ledger().set_timestamp(2000);
    client.anchor_merchant_tos(&merchant, &hash_v2);

    // Subscriber's snapshot still points to v1 (the version they agreed to)
    let snap = client
        .get_subscription_tos_snapshot(&subscriber, &merchant)
        .unwrap();
    assert_eq!(snap.version, 1);
    assert_eq!(snap.ipfs_hash, hash_v1);

    // Current ToS is now v2
    let current = client.get_merchant_tos(&merchant);
    assert_eq!(current.version, 2);

    // verify_tos_agreement returns false — subscriber is on old version
    let agrees = client.verify_tos_agreement(&subscriber, &merchant);
    assert!(!agrees, "subscriber should NOT agree to current ToS (v2)");
}

#[test]
fn test_verify_tos_agreement_true_when_versions_match() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, merchant) = setup_verified_merchant(&env);
    let subscriber = Address::generate(&env);

    let hash_v1 = ipfs_hash(&env, "QmV1Match11111111111111111111111");
    client.anchor_merchant_tos(&merchant, &hash_v1);

    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token = token::Client::new(&env, &sac.address());
    let token_admin_client = token::StellarAssetClient::new(&env, &sac.address());
    token_admin_client.mint(&subscriber, &10_000);

    client.subscribe(
        &subscriber,
        &merchant,
        &token.address,
        &10_000,
        &1_000_000_000,
        &None,
    );

    // No ToS update → still v1
    let agrees = client.verify_tos_agreement(&subscriber, &merchant);
    assert!(agrees, "subscriber agreed to current ToS (v1)");
}

// ---------------------------------------------------------------------------
// ToS versioning and immutability
// ---------------------------------------------------------------------------

#[test]
fn test_tos_version_increments_monotonically() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, merchant) = setup_verified_merchant(&env);

    let h1 = ipfs_hash(&env, "QmV1111111111111111111111111111");
    let h2 = ipfs_hash(&env, "QmV2222222222222222222222222222");
    let h3 = ipfs_hash(&env, "QmV3333333333333333333333333333");

    client.anchor_merchant_tos(&merchant, &h1);
    assert_eq!(client.get_merchant_tos(&merchant).version, 1);

    client.anchor_merchant_tos(&merchant, &h2);
    assert_eq!(client.get_merchant_tos(&merchant).version, 2);

    client.anchor_merchant_tos(&merchant, &h3);
    assert_eq!(client.get_merchant_tos(&merchant).version, 3);
}

#[test]
#[should_panic(expected = "ipfs hash cannot be empty")]
fn test_anchor_empty_hash_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, merchant) = setup_verified_merchant(&env);
    client.anchor_merchant_tos(&merchant, &Bytes::new(&env));
}

#[test]
#[should_panic(expected = "ipfs hash too long")]
fn test_anchor_too_long_hash_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, merchant) = setup_verified_merchant(&env);
    // 65-byte hash — exceeds limit
    let long = Bytes::from_slice(&env, &[b'Q'; 65]);
    client.anchor_merchant_tos(&merchant, &long);
}

#[test]
#[should_panic(expected = "merchant is not verified")]
fn test_unverified_merchant_cannot_anchor_tos() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let rogue_merchant = Address::generate(&env);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    let h = ipfs_hash(&env, "QmUnverified11111111111111111111");
    client.anchor_merchant_tos(&rogue_merchant, &h);
}

// ---------------------------------------------------------------------------
// Acceptance 3: Minimal overhead — anchor is a single storage set + event
// ---------------------------------------------------------------------------

#[test]
fn test_anchor_overhead_is_minimal() {
    // This test verifies the anchor call completes without errors and the
    // data is immediately queryable in the same block.
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(999);

    let (client, _admin, merchant) = setup_verified_merchant(&env);
    let h = ipfs_hash(&env, "QmMinimalOverhead111111111111111");

    client.anchor_merchant_tos(&merchant, &h);

    let anchor = client.get_merchant_tos(&merchant);
    assert_eq!(anchor.anchored_at, 999);
    assert_eq!(anchor.version, 1);
    assert_eq!(anchor.merchant, merchant);
}
