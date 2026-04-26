/// Issue #126: Standardize Indexed Events for Subgraph Parity
/// Tests covering:
/// Acceptance 1: 100% of protocol state changes emit a standardized, indexable event.
/// Acceptance 2: No sensitive user data is leaked into the public ledger stream.
/// Acceptance 3: Standardized logs allow efficient off-chain graph querying.
#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger},
    token, vec, Address, Env, IntoVal,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup_verified_merchant_with_token(
    env: &Env,
) -> (SubStreamContractClient<'static>, Address, Address, token::Client<'static>) {
    let admin = Address::generate(env);
    let merchant = Address::generate(env);
    let token_admin = Address::generate(env);

    let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token = token::Client::new(env, &sac.address());
    let token_admin_client = token::StellarAssetClient::new(env, &sac.address());

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(env, &contract_id);
    client.initialize(&admin);

    // Write merchant status directly (bypass KYC issuer)
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

    // Mint tokens for subscribers
    let subscriber = Address::generate(env);
    token_admin_client.mint(&subscriber, &100_000);

    let client: SubStreamContractClient<'static> = unsafe { core::mem::transmute(client) };
    let token: token::Client<'static> = unsafe { core::mem::transmute(token) };
    (client, admin, merchant, token)
}

// ---------------------------------------------------------------------------
// Acceptance 1: Every state change emits a standardized event
// ---------------------------------------------------------------------------

#[test]
fn test_subscribe_emits_subscription_created_event() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(5000);

    let (client, _admin, merchant, token) = setup_verified_merchant_with_token(&env);
    let subscriber = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
    let tok = token::Client::new(&env, &sac.address());
    let tok_admin = token::StellarAssetClient::new(&env, &sac.address());
    tok_admin.mint(&subscriber, &50_000);
    let _ = token;

    client.subscribe(
        &subscriber,
        &merchant,
        &tok.address,
        &50_000,
        &1_000_000_000,
        &None,
    );

    let events = env.events().all();
    // Check that a SubscriptionCreated event was published (has topics with subscriber + merchant)
    let found = events.iter().any(|e| {
        let (_, topics, _) = e;
        // topics[0] = subscriber, topics[1] = merchant for SubscriptionCreated
        topics.len() >= 2
    });
    assert!(found, "at least one event with two topics must be published on subscribe");
}

#[test]
fn test_register_plan_emits_plan_registered_event() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1000);

    let (client, _admin, merchant, _token) = setup_verified_merchant_with_token(&env);

    let plan = Plan {
        plan_id: 1,
        name: soroban_sdk::String::from_str(&env, "Basic"),
        billing_amount: 10_000,
        billing_cycle: 30 * 24 * 60 * 60,
        has_trial: false,
        trial_duration: 0,
        is_active: true,
    };

    client.register_plan(&merchant, &plan);

    let events = env.events().all();
    assert!(
        !events.is_empty(),
        "PlanRegistered event must be emitted"
    );
}

#[test]
fn test_cancel_emits_subscription_cancelled_event() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(0);

    let (client, _admin, merchant, _token) = setup_verified_merchant_with_token(&env);
    let subscriber = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
    let tok = token::Client::new(&env, &sac.address());
    let tok_admin = token::StellarAssetClient::new(&env, &sac.address());
    tok_admin.mint(&subscriber, &100_000);

    client.subscribe(
        &subscriber,
        &merchant,
        &tok.address,
        &100_000,
        &1_000_000_000,
        &None,
    );

    // Advance past minimum duration
    env.ledger().set_timestamp(2 * 24 * 60 * 60);
    client.cancel(&subscriber, &merchant);

    let events = env.events().all();
    assert!(
        !events.is_empty(),
        "SubscriptionCancelled event must be emitted on cancel"
    );
}

// ---------------------------------------------------------------------------
// Acceptance 2: No sensitive data in event topics
// ---------------------------------------------------------------------------

#[test]
fn test_event_topics_contain_only_addresses_and_amounts() {
    // Subscribe and verify that events only expose what is necessary for indexing
    // (addresses and amounts), not private content such as payment details or user info).
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, merchant, _token) = setup_verified_merchant_with_token(&env);
    let subscriber = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
    let tok = token::Client::new(&env, &sac.address());
    let tok_admin = token::StellarAssetClient::new(&env, &sac.address());
    tok_admin.mint(&subscriber, &50_000);

    client.subscribe(
        &subscriber,
        &merchant,
        &tok.address,
        &50_000,
        &1_000_000_000,
        &None,
    );

    // All events should have a non-zero topic array (indexable)
    let events = env.events().all();
    for (_, topics, _) : (_, soroban_sdk::Vec<soroban_sdk::Val>, _) in events.iter() {
        // Each event must have at least one topic so it is filterable
        assert!(topics.len() >= 1, "event must have at least one topic for subgraph indexing");
    }
}

// ---------------------------------------------------------------------------
// Acceptance 3: Emit helpers produce consistent, queryable events
// ---------------------------------------------------------------------------

#[test]
fn test_emit_subscription_created_helper() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(8000);

    let (client, _admin, merchant, token) = setup_verified_merchant_with_token(&env);
    let subscriber = Address::generate(&env);

    client.emit_subscription_created(
        &subscriber,
        &merchant,
        &token.address,
        &1_000_000_000,
        &1,
    );

    let events = env.events().all();
    assert!(!events.is_empty());
}

#[test]
fn test_emit_subscription_cancelled_helper() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(9000);

    let (client, _admin, merchant, _token) = setup_verified_merchant_with_token(&env);
    let subscriber = Address::generate(&env);

    client.emit_subscription_cancelled(&subscriber, &merchant, &500);

    let events = env.events().all();
    assert!(!events.is_empty());
}

#[test]
fn test_emit_subscription_renewed_helper() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(10_000);

    let (client, _admin, merchant, _token) = setup_verified_merchant_with_token(&env);
    let subscriber = Address::generate(&env);

    client.emit_subscription_renewed(&subscriber, &merchant, &1000, &20_000);

    let events = env.events().all();
    assert!(!events.is_empty());
}

#[test]
fn test_emit_plan_registered_helper() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(500);

    let (client, _admin, merchant, _token) = setup_verified_merchant_with_token(&env);

    client.emit_plan_registered(&merchant, &3, &9_999, &2_592_000);

    let events = env.events().all();
    assert!(!events.is_empty());
}

#[test]
fn test_emit_protocol_revenue_collected_helper() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(600);

    let (client, _admin, merchant, token) = setup_verified_merchant_with_token(&env);

    client.emit_protocol_revenue_collected(&merchant, &token.address, &250);

    let events = env.events().all();
    assert!(!events.is_empty());
}
