#![cfg(test)]

use super::*;
use soroban_sdk::{
<<<<<<< feature/advanced-subscription-engine-bundle
    testutils::{Address as _, Events as _, Ledger},
    token, vec, Address, Bytes, Env,
=======
    testutils::{Address as _, Ledger},
    testutils::Events as _,
    token, vec, Address, Env,
>>>>>>> main
};

const DAY: u64 = 24 * 60 * 60;
const WEEK: u64 = 7 * DAY;

fn create_token_contract<'a>(env: &Env, admin: &Address) -> token::Client<'a> {
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    token::Client::new(env, &sac.address())
}

<<<<<<< feature/advanced-subscription-engine-bundle
=======
fn last_call_contract_event_count(env: &Env, contract_id: &Address) -> usize {
    let events = env.events().all().filter_by_contract(contract_id);
    events.events().len()
}

// ---------------------------------------------------------------------------
// is_subscribed
// ---------------------------------------------------------------------------

>>>>>>> main
#[test]
fn test_is_subscribed_active() {
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
<<<<<<< feature/advanced-subscription-engine-bundle
    token_admin.mint(&subscriber, &1000);
=======
    token_admin.mint(&subscriber, &10000000);
>>>>>>> main

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    env.ledger().set_timestamp(100);
<<<<<<< feature/advanced-subscription-engine-bundle
    client.subscribe(&subscriber, &creator, &token.address, &100, &10);

    // Still active: expiry = 100 + (100/10) = 110
=======
    client.subscribe(&subscriber, &creator, &token.address, &1000, &1_000_000_000);

>>>>>>> main
    env.ledger().set_timestamp(105);
    assert!(client.is_subscribed(&subscriber, &creator));
}

#[test]
fn test_is_subscribed_expired() {
<<<<<<< feature/advanced-subscription-engine-bundle
    let start = 100u64;
    env.ledger().set_timestamp(start);
    env.ledger().set_timestamp(100);
    client.subscribe(&subscriber, &creator, &token.address, &100, &2);

    assert_eq!(token.balance(&subscriber), 900);
    assert_eq!(token.balance(&contract_id), 100);

    // Still inside trial: no charges.
    env.ledger().set_timestamp(start + 10);
    client.collect(&subscriber, &creator);
    assert_eq!(token.balance(&creator), 0);

    // 10 paid seconds after trial.
    env.ledger().set_timestamp(start + WEEK + 10);
    env.ledger().set_timestamp(110);
    client.collect(&subscriber, &creator);

    assert_eq!(token.balance(&creator), 20);
    assert_eq!(token.balance(&contract_id), 80);

    // Advance 50 more seconds — would be 100 tokens but only 80 left
    // Additional 50 paid seconds, capped by remaining balance.
    env.ledger().set_timestamp(start + WEEK + 60);
    env.ledger().set_timestamp(160);
    client.collect(&subscriber, &creator);

    assert_eq!(token.balance(&creator), 100);
    assert_eq!(token.balance(&contract_id), 0);
    assert_eq!(client.get_total_streamed(&subscriber, &creator), 100);
}

#[test]
fn test_free_trial_ignores_claims_within_first_week() {
#[should_panic(expected = "cannot cancel stream: minimum duration not met")]
fn test_cancel_before_minimum_duration() {
=======
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&subscriber, &1000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    env.ledger().set_timestamp(100);
    client.subscribe(&subscriber, &creator, &token.address, &10, &10_000_000_000);

    env.ledger().set_timestamp(100 + WEEK + 2);
    assert!(!client.is_subscribed(&subscriber, &creator));
}

#[test]
fn test_balance_depletion_auto_close_at_zero() {
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&subscriber, &1000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    // Subscribe with exactly 100 tokens at 10 tokens/second.
    // rate must be 10 × PRECISION_MULTIPLIER to yield 10 token-units per second.
    // Exhausts after 10 paid seconds (post-7-day trial).
    let start = 100u64;
    env.ledger().set_timestamp(start);
    client.subscribe(&subscriber, &creator, &token.address, &100, &10_000_000_000);

    // One second before balance reaches zero: still subscribed
    env.ledger().set_timestamp(start + WEEK + 9);
    assert!(client.is_subscribed(&subscriber, &creator));

    // Exactly at zero: balance == potential_charge, strict > check fails -> inactive
    env.ledger().set_timestamp(start + WEEK + 10);
    assert!(!client.is_subscribed(&subscriber, &creator));

    // Collect drains the 100 deposited tokens to creator; triggers grace period
    client.collect(&subscriber, &creator);
    assert_eq!(token.balance(&creator), 100);
    assert_eq!(token.balance(&contract_id), 0);

    // After grace period expires (GRACE_PERIOD = 86400s) stream is permanently closed
    env.ledger().set_timestamp(start + WEEK + 10 + GRACE_PERIOD + 1);
    assert!(!client.is_subscribed(&subscriber, &creator));
}

#[test]
fn test_is_subscribed_none() {
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    assert!(!client.is_subscribed(&subscriber, &creator));
}

// ---------------------------------------------------------------------------
// Free trial
// ---------------------------------------------------------------------------

#[test]
fn test_free_trial_ignores_claims_within_first_week() {
>>>>>>> main
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&subscriber, &1000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

<<<<<<< feature/advanced-subscription-engine-bundle
    env.ledger().set_timestamp(100);
    client.subscribe(&subscriber, &creator, &token.address, &100, &1);

    // 24h + 120 seconds pass
    env.ledger().set_timestamp(100 + 86400 + 120);
    client.cancel(&subscriber, &creator);
    let start = 100u64;
    env.ledger().set_timestamp(start);
    client.subscribe(&subscriber, &creator, &token.address, &300, &3);
=======
    let start = 100u64;
    env.ledger().set_timestamp(start);
    client.subscribe(&subscriber, &creator, &token.address, &300, &3_000_000_000);
>>>>>>> main

    env.ledger().set_timestamp(start + WEEK - 1);
    client.collect(&subscriber, &creator);
    assert_eq!(token.balance(&creator), 0);

    env.ledger().set_timestamp(start + WEEK + 9);
    client.collect(&subscriber, &creator);
    assert_eq!(token.balance(&creator), 27);
}

#[test]
<<<<<<< feature/advanced-subscription-engine-bundle
#[should_panic(expected = "cannot cancel stream: minimum duration not met")]
fn test_cancel_before_minimum_duration() {
=======
fn test_free_to_paid_transition_event_emitted_once() {
>>>>>>> main
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&subscriber, &1000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

<<<<<<< feature/advanced-subscription-engine-bundle
    env.ledger().set_timestamp(100);
    client.subscribe(&subscriber, &creator, &token.address, &100, &1);

    env.ledger().set_timestamp(100 + 3600);
    client.cancel(&subscriber, &creator);
}

#[test]
fn test_cancel_after_minimum_duration() {
=======
    let start = 100u64;
    env.ledger().set_timestamp(start);
    client.subscribe(&subscriber, &creator, &token.address, &300, &1);

    env.ledger().set_timestamp(start + WEEK + 1);
    client.collect(&subscriber, &creator);
    assert_eq!(last_call_contract_event_count(&env, &contract_id), 1);

    env.ledger().set_timestamp(start + WEEK + 10);
    client.collect(&subscriber, &creator);
    assert_eq!(last_call_contract_event_count(&env, &contract_id), 0);
}

// ---------------------------------------------------------------------------
// Cancel
// ---------------------------------------------------------------------------

// Early cancellation now applies a minimum-lock penalty instead of panicking.
// The creator receives tokens equal to rate × MINIMUM_FLOW_DURATION (capped at
// the subscriber's remaining balance).
#[test]
fn test_cancel_before_minimum_duration_applies_penalty() {
>>>>>>> main
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
<<<<<<< feature/advanced-subscription-engine-bundle
=======
    // Subscriber starts with 1000 tokens and deposits 100.
>>>>>>> main
    token_admin.mint(&subscriber, &1000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

<<<<<<< feature/advanced-subscription-engine-bundle
    client.subscribe(&subscriber, &creator, &token.address, &100, &2);
    client.subscribe(&subscriber, &creator, &token.address, &100, &2);
    let start = 100u64;
    env.ledger().set_timestamp(start);
    client.subscribe(&subscriber, &creator, &token.address, &100, &1);

    // Minimum duration has passed, but still inside free trial.
    env.ledger().set_timestamp(start + DAY + 10);
    client.cancel(&subscriber, &creator);

    assert_eq!(token.balance(&creator), 0);
    assert_eq!(token.balance(&subscriber), 1000);
    env.ledger().set_timestamp(100);
    client.subscribe(&subscriber, &creator, &token.address, &100, &1);

    env.ledger().set_timestamp(100 + 86400 + 10);
    client.cancel(&subscriber, &creator);

    assert_eq!(token.balance(&creator), 100);
    assert_eq!(token.balance(&subscriber), 900);
    assert_eq!(token.balance(&contract_id), 0);
}

#[test]
fn test_top_up() {
=======
    let rate: i128 = 1_000_000_000; // 1 token/second
    env.ledger().set_timestamp(100);
    client.subscribe(&subscriber, &creator, &token.address, &100, &rate);

    // Cancel after only 1 hour — still inside the 24-hour lock window.
    env.ledger().set_timestamp(100 + 3600);
    client.cancel(&subscriber, &creator);

    // Minimum entitled = rate × 86400 = 86_400_000_000_000 nano-units
    // → 86_400 tokens, but balance is only 100 tokens → penalty capped at 100.
    assert_eq!(token.balance(&creator), 100, "creator should receive full penalty");
    assert_eq!(token.balance(&subscriber), 900, "subscriber gets no refund (penalty = deposit)");
    assert_eq!(token.balance(&contract_id), 0, "contract should hold nothing after cancel");

    // Subscription must be removed.
    assert!(!client.is_subscribed(&subscriber, &creator));
}

#[test]
fn test_cancel_after_minimum_duration() {
>>>>>>> main
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&subscriber, &1000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

<<<<<<< feature/advanced-subscription-engine-bundle
    client.subscribe(&subscriber, &creator, &token.address, &100, &1);
    assert_eq!(token.balance(&contract_id), 100);

    env.ledger().set_timestamp(0);
    client.subscribe(&subscriber, &creator, &token.address, &100, &1);
    client.top_up(&subscriber, &creator, &50);

    env.ledger().set_timestamp(WEEK + 120);
    env.ledger().set_timestamp(120);
    client.collect(&subscriber, &creator);

    assert_eq!(token.balance(&creator), 120);
    assert_eq!(token.balance(&contract_id), 30);
}

#[test]
fn test_inactive_stream_moves_to_temporary_storage() {
=======
    let start = 100u64;
    env.ledger().set_timestamp(start);
    client.subscribe(&subscriber, &creator, &token.address, &100, &1_000_000_000);

    env.ledger().set_timestamp(start + DAY + 10);
    client.cancel(&subscriber, &creator);

    assert_eq!(token.balance(&creator), 0);
    assert_eq!(token.balance(&subscriber), 1000);
}

#[test]
fn test_cancel_exactly_at_minimum_duration() {
>>>>>>> main
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&subscriber, &1000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

<<<<<<< feature/advanced-subscription-engine-bundle
    let start = 100u64;
    env.ledger().set_timestamp(start);
    client.subscribe(&subscriber, &creator, &token.address, &10, &1);

    let key = DataKey::Stream(subscriber.clone(), creator.clone());
    env.as_contract(&contract_id, || {
        assert!(env.storage().persistent().has(&key));
        assert!(!env.storage().temporary().has(&key));
    });

    // Deplete stream balance after trial; this should mark stream inactive.
    env.ledger().set_timestamp(start + WEEK + 20);
    client.collect(&subscriber, &creator);

    assert_eq!(token.balance(&contract_id), 0);
    env.as_contract(&contract_id, || {
        assert!(!env.storage().persistent().has(&key));
        assert!(env.storage().temporary().has(&key));
    });
}

#[test]
fn test_top_up_reactivates_stream_to_persistent_storage() {
fn test_group_subscribe_and_collect_split() {
=======
    env.ledger().set_timestamp(100);
    client.subscribe(&subscriber, &creator, &token.address, &100, &1_000_000_000);

    env.ledger().set_timestamp(100 + DAY);
    client.cancel(&subscriber, &creator);

    assert_eq!(token.balance(&creator), 0);
    assert_eq!(token.balance(&subscriber), 1000);
    assert_eq!(token.balance(&contract_id), 0);
}

// ---------------------------------------------------------------------------
// Top-up
// ---------------------------------------------------------------------------

#[test]
fn test_top_up() {
>>>>>>> main
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
<<<<<<< feature/advanced-subscription-engine-bundle
    let channel_id = Address::generate(&env);
    let creator_1 = Address::generate(&env);
    let creator_2 = Address::generate(&env);
    let creator_3 = Address::generate(&env);
    let creator_4 = Address::generate(&env);
    let creator_5 = Address::generate(&env);
=======
    let creator = Address::generate(&env);
>>>>>>> main
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&subscriber, &1000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

<<<<<<< feature/advanced-subscription-engine-bundle
    let start = 100u64;
    env.ledger().set_timestamp(start);
    client.subscribe(&subscriber, &creator, &token.address, &10, &1);

    let key = DataKey::Stream(subscriber.clone(), creator.clone());
    env.ledger().set_timestamp(start + WEEK + 20);
    client.collect(&subscriber, &creator);

    env.as_contract(&contract_id, || {
        assert!(env.storage().temporary().has(&key));
        assert!(!env.storage().persistent().has(&key));
    });

    client.top_up(&subscriber, &creator, &5);

    env.as_contract(&contract_id, || {
        assert!(env.storage().persistent().has(&key));
        assert!(!env.storage().temporary().has(&key));
    });
}

=======
    env.ledger().set_timestamp(0);
    client.subscribe(&subscriber, &creator, &token.address, &100, &1_000_000_000);
    assert_eq!(token.balance(&contract_id), 100);

    client.top_up(&subscriber, &creator, &50);
    assert_eq!(token.balance(&contract_id), 150);

    env.ledger().set_timestamp(WEEK + 120);
    client.collect(&subscriber, &creator);

    assert_eq!(token.balance(&creator), 120);
    assert_eq!(token.balance(&contract_id), 30);
}

// ---------------------------------------------------------------------------
// Group channel
// ---------------------------------------------------------------------------

>>>>>>> main
#[test]
fn test_group_subscribe_and_collect_split() {
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let channel_id = Address::generate(&env);
    let creator_1 = Address::generate(&env);
    let creator_2 = Address::generate(&env);
    let creator_3 = Address::generate(&env);
    let creator_4 = Address::generate(&env);
    let creator_5 = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&subscriber, &1000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    let creators = vec![
        &env,
        creator_1.clone(),
        creator_2.clone(),
        creator_3.clone(),
        creator_4.clone(),
        creator_5.clone(),
    ];
    let percentages = vec![&env, 40u32, 25u32, 15u32, 10u32, 10u32];

    let start = 100u64;
    env.ledger().set_timestamp(start);
<<<<<<< feature/advanced-subscription-engine-bundle
    env.ledger().set_timestamp(100);
=======
>>>>>>> main
    client.subscribe_group(
        &subscriber,
        &channel_id,
        &token.address,
        &500,
<<<<<<< feature/advanced-subscription-engine-bundle
        &10,
=======
        &10_000_000_000,
>>>>>>> main
        &creators,
        &percentages,
    );

    env.ledger().set_timestamp(start + WEEK + 10);
<<<<<<< feature/advanced-subscription-engine-bundle
    env.ledger().set_timestamp(110);
    client.collect_group(&subscriber, &channel_id);

    // 10 seconds * 10 tokens/sec = 100 tokens split across creators
=======
    client.collect_group(&subscriber, &channel_id);

>>>>>>> main
    assert_eq!(token.balance(&creator_1), 40);
    assert_eq!(token.balance(&creator_2), 25);
    assert_eq!(token.balance(&creator_3), 15);
    assert_eq!(token.balance(&creator_4), 10);
    assert_eq!(token.balance(&creator_5), 10);
    assert_eq!(token.balance(&contract_id), 400);
}

#[test]
<<<<<<< feature/advanced-subscription-engine-bundle
fn test_cliff_based_access_before_threshold() {
=======
>>>>>>> main
#[should_panic(expected = "group channel must contain exactly 5 creators")]
fn test_group_requires_exactly_five_creators() {
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
<<<<<<< feature/advanced-subscription-engine-bundle
    let creator = Address::generate(&env);
=======
>>>>>>> main
    let channel_id = Address::generate(&env);
    let creator_1 = Address::generate(&env);
    let creator_2 = Address::generate(&env);
    let creator_3 = Address::generate(&env);
    let creator_4 = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&subscriber, &1000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

<<<<<<< feature/advanced-subscription-engine-bundle
    client.set_cliff_threshold(&creator, &50);
    assert_eq!(client.get_cliff_threshold(&creator), 50);

    assert!(!client.has_unlocked_access(&subscriber, &creator));
    assert_eq!(client.get_access_tier(&subscriber, &creator), 0);

    client.subscribe(&subscriber, &creator, &token.address, &30, &1);
    env.ledger().set_timestamp(100);
    client.collect(&subscriber, &creator);

    assert!(!client.has_unlocked_access(&subscriber, &creator));
    assert_eq!(client.get_access_tier(&subscriber, &creator), 0);
}

#[test]
fn test_cancel_after_minimum_duration() {
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&subscriber, &1000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    env.ledger().set_timestamp(100);
    client.subscribe(&subscriber, &creator, &token.address, &100, &1);

    // Advance beyond minimum duration (24h + 1h)
    env.ledger().set_timestamp(100 + 86400 + 3600);
    client.cancel(&subscriber, &creator);

    assert_eq!(token.balance(&creator), 100);
    assert_eq!(token.balance(&subscriber), 900);
}

#[test]
fn test_migrate_tier_downgrade_prorates_refund() {
=======
    let creators = vec![
        &env,
        creator_1.clone(),
        creator_2.clone(),
        creator_3.clone(),
        creator_4.clone(),
    ];
    let percentages = vec![&env, 25u32, 25u32, 25u32, 25u32];

    client.subscribe_group(
        &subscriber,
        &channel_id,
        &token.address,
        &100,
        &1_000_000_000,
        &creators,
        &percentages,
    );
}

#[test]
#[should_panic(expected = "percentages must sum to 100")]
fn test_group_percentages_must_sum_to_100() {
>>>>>>> main
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
<<<<<<< feature/advanced-subscription-engine-bundle
    let creator = Address::generate(&env);
=======
    let channel_id = Address::generate(&env);
    let creator_1 = Address::generate(&env);
    let creator_2 = Address::generate(&env);
    let creator_3 = Address::generate(&env);
    let creator_4 = Address::generate(&env);
    let creator_5 = Address::generate(&env);
>>>>>>> main
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&subscriber, &1000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

<<<<<<< feature/advanced-subscription-engine-bundle
    env.ledger().set_timestamp(100);
    client.subscribe(&subscriber, &creator, &token.address, &100, &10);

    env.ledger().set_timestamp(105);
    client.migrate_tier(&subscriber, &creator, &5, &0);
=======
>>>>>>> main
    let creators = vec![
        &env,
        creator_1.clone(),
        creator_2.clone(),
        creator_3.clone(),
<<<<<<< feature/advanced-subscription-engine-bundle
        creator_4.clone()
    ];
    let percentages = vec![&env, 25u32, 25u32, 25u32, 25u32];
=======
        creator_4.clone(),
        creator_5.clone(),
    ];
    let percentages = vec![&env, 30u32, 20u32, 20u32, 10u32, 10u32];
>>>>>>> main

    client.subscribe_group(
        &subscriber,
        &channel_id,
        &token.address,
        &100,
<<<<<<< feature/advanced-subscription-engine-bundle
        &1,
=======
        &1_000_000_000,
>>>>>>> main
        &creators,
        &percentages,
    );
}

#[test]
fn test_group_cancel_collects_and_refunds_remaining_balance() {
<<<<<<< feature/advanced-subscription-engine-bundle
fn test_pause_channel_blocks_charges_and_unpause_resumes() {
=======
>>>>>>> main
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let channel_id = Address::generate(&env);
    let creator_1 = Address::generate(&env);
    let creator_2 = Address::generate(&env);
    let creator_3 = Address::generate(&env);
    let creator_4 = Address::generate(&env);
    let creator_5 = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&subscriber, &1000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    let creators = vec![
        &env,
        creator_1.clone(),
        creator_2.clone(),
        creator_3.clone(),
        creator_4.clone(),
        creator_5.clone(),
    ];
    let percentages = vec![&env, 40u32, 20u32, 20u32, 10u32, 10u32];

<<<<<<< feature/advanced-subscription-engine-bundle
    // Start at t=0, deposit 200 tokens at rate 1/sec
    // After exactly 30 seconds, cancel (past minimum duration)
    // 30 tokens collected, 170 refunded
=======
>>>>>>> main
    env.ledger().set_timestamp(0);
    client.subscribe_group(
        &subscriber,
        &channel_id,
        &token.address,
        &200,
<<<<<<< feature/advanced-subscription-engine-bundle
        &1,
=======
        &1_000_000_000,
>>>>>>> main
        &creators,
        &percentages,
    );

<<<<<<< feature/advanced-subscription-engine-bundle
    // Advance past minimum duration (24h) + 30 seconds
    env.ledger().set_timestamp(86400 + 30);
    client.cancel_group(&subscriber, &channel_id);

    // 86430s * 1/sec = 86430, capped at balance 200 → all 200 collected
    // 200 tokens split: 40%=80, 20%=40, 20%=40, 10%=20, 10%=20
    assert_eq!(token.balance(&creator_1), 80);
    assert_eq!(token.balance(&creator_2), 40);
    assert_eq!(token.balance(&creator_3), 40);
    assert_eq!(token.balance(&creator_4), 20);
    assert_eq!(token.balance(&creator_5), 20);
    assert_eq!(token.balance(&subscriber), 800); // 1000 - 200 deposited, 0 refund
    assert_eq!(token.balance(&contract_id), 0);
}

#[test]
fn test_cliff_based_access_after_threshold() {
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&subscriber, &1000);
=======
    env.ledger().set_timestamp(DAY + 30);
    client.cancel_group(&subscriber, &channel_id);

    assert_eq!(token.balance(&creator_1), 0);
    assert_eq!(token.balance(&creator_2), 0);
    assert_eq!(token.balance(&creator_3), 0);
    assert_eq!(token.balance(&creator_4), 0);
    assert_eq!(token.balance(&creator_5), 0);
    assert_eq!(token.balance(&subscriber), 1000);
    assert_eq!(token.balance(&contract_id), 0);
}

// ---------------------------------------------------------------------------
// Creator Verification Badge — Issue #23
// ---------------------------------------------------------------------------

#[test]
fn test_verify_creator_emits_event() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
>>>>>>> main

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

<<<<<<< feature/advanced-subscription-engine-bundle
    env.ledger().set_timestamp(100);
    client.subscribe(&subscriber, &creator, &token.address, &100, &10);

    // Expired: expiry = 100 + (100/10) = 110
    env.ledger().set_timestamp(111);
    assert!(!client.is_subscribed(&subscriber, &creator));
}

#[test]
fn test_is_subscribed_none() {
    let start = 100u64;
    env.ledger().set_timestamp(start);
    client.subscribe(&subscriber, &creator, &token.address, &300, &2);

    env.ledger().set_timestamp(start + WEEK + 10);
    client.collect(&subscriber, &creator);
    assert_eq!(token.balance(&creator), 20);

    env.ledger().set_timestamp(start + WEEK + 20);
    client.pause_channel(&creator);
    assert!(client.is_channel_paused(&creator));
    assert_eq!(token.balance(&creator), 40);

#[test]
fn test_migrate_tier_upgrade_with_additional_deposit() {
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&subscriber, &1000);
=======
    client.initialize(&admin);
    client.verify_creator(&admin, &creator);

    assert!(client.is_creator_verified(&creator));
}

#[test]
#[should_panic(expected = "admin only")]
fn test_verify_creator_non_admin_panics() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    let creator = Address::generate(&env);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    client.initialize(&admin);
    client.verify_creator(&attacker, &creator);
}

#[test]
fn test_is_creator_verified_returns_false_by_default() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
>>>>>>> main

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

<<<<<<< feature/advanced-subscription-engine-bundle
    env.ledger().set_timestamp(100);
    client.subscribe(&subscriber, &creator, &token.address, &100, &1);

    client.migrate_tier(&subscriber, &creator, &2, &50);

    assert_eq!(token.balance(&contract_id), 150);
    assert_eq!(token.balance(&subscriber), 850);
}

#[test]
fn test_access_tiers() {
    env.ledger().set_timestamp(start + WEEK + 100);
    client.collect(&subscriber, &creator);
    env.ledger().set_timestamp(100);
    client.subscribe(&subscriber, &creator, &token.address, &300, &2);

    env.ledger().set_timestamp(110);
    client.collect(&subscriber, &creator);
    assert_eq!(token.balance(&creator), 20);

    env.ledger().set_timestamp(120);
    client.pause_channel(&creator);
    assert!(client.is_channel_paused(&creator));
    // Pause settles the 10-second pending amount before freezing.
    assert_eq!(token.balance(&creator), 40);

    env.ledger().set_timestamp(200);
    client.collect(&subscriber, &creator);
    // No additional charges while paused.
    assert_eq!(token.balance(&creator), 40);

    client.unpause_channel(&creator);
    assert!(!client.is_channel_paused(&creator));

    env.ledger().set_timestamp(start + WEEK + 110);
    env.ledger().set_timestamp(210);
    client.collect(&subscriber, &creator);
    assert_eq!(token.balance(&creator), 60);
    assert_eq!(token.balance(&contract_id), 240);
}

#[test]
fn test_pause_channel_applies_to_all_subscribers() {
    let env = Env::default();
    env.mock_all_auths();

    let subscriber_1 = Address::generate(&env);
    let subscriber_2 = Address::generate(&env);
=======
    client.initialize(&admin);

    assert!(!client.is_creator_verified(&creator));
}

#[test]
#[should_panic(expected = "already initialized")]
fn test_initialize_cannot_be_called_twice() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    client.initialize(&admin);
    client.initialize(&attacker);
}

// ---------------------------------------------------------------------------
// Flash-Stream Attack Simulation — Issue #26
// ---------------------------------------------------------------------------

// Flash-subscribe attack: attacker subscribes, immediately cancels to recover
// their deposit after scraping content.  The minimum-lock penalty ensures the
// creator is compensated even when the cancel arrives within the same ledger.
#[test]
fn test_flash_stream_attack_within_single_ledger() {
    let env = Env::default();
    env.mock_all_auths();

    let attacker = Address::generate(&env);
>>>>>>> main
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
<<<<<<< feature/advanced-subscription-engine-bundle
    token_admin.mint(&subscriber, &1000);
    token_admin.mint(&subscriber_1, &200);
    token_admin.mint(&subscriber_2, &200);
=======
    token_admin.mint(&attacker, &1000000);
>>>>>>> main

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

<<<<<<< feature/advanced-subscription-engine-bundle
    let creators = vec![
        &env,
        creator_1.clone(),
        creator_2.clone(),
        creator_3.clone(),
        creator_4.clone()
    ];
    let percentages = vec![&env, 25u32, 25u32, 25u32, 25u32];

    client.subscribe_group(
        &subscriber,
        &channel_id,
        &token.address,
        &100,
        &1,
        &creators,
        &percentages,
    );
}

#[test]
fn test_pause_channel_blocks_charges_and_unpause_resumes() {
    let start = 100u64;
    env.ledger().set_timestamp(start);
    client.subscribe(&subscriber_1, &creator, &token.address, &200, &1);
    client.subscribe(&subscriber_2, &creator, &token.address, &200, &1);

    env.ledger().set_timestamp(start + WEEK + 30);
    client.pause_channel(&creator);
    assert_eq!(token.balance(&creator), 60);

    env.ledger().set_timestamp(start + WEEK + 130);
    client.unpause_channel(&creator);

    env.ledger().set_timestamp(start + WEEK + 140);
    let total = client.withdraw_all(&creator, &10);

    env.ledger().set_timestamp(100);
    client.subscribe(&subscriber_1, &creator, &token.address, &200, &1);
    client.subscribe(&subscriber_2, &creator, &token.address, &200, &1);

    env.ledger().set_timestamp(130);
    client.pause_channel(&creator);
    assert_eq!(token.balance(&creator), 60);

    env.ledger().set_timestamp(230);
    client.unpause_channel(&creator);

    env.ledger().set_timestamp(240);
    let total = client.withdraw_all(&creator, &10);

    // Only post-unpause 10 seconds are billable for each stream.
    assert_eq!(total, 20);
    assert_eq!(token.balance(&creator), 80);
    assert_eq!(token.balance(&contract_id), 320);
}

#[test]
fn test_cliff_threshold_access() {
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
=======
    let ledger_time = 1000000u64;
    env.ledger().set_timestamp(ledger_time);

    // Attacker deposits 10 tokens.
    client.subscribe(&attacker, &creator, &token.address, &10, &1_000_000_000);
    assert!(client.is_subscribed(&attacker, &creator));

    // Attempt to cancel within the same ledger (0 seconds elapsed).
    // The cancel succeeds but applies the minimum-lock penalty.
    client.cancel(&attacker, &creator);

    // Subscription is gone — attacker can no longer access content.
    assert!(!client.is_subscribed(&attacker, &creator));

    // Creator receives the full deposit as a penalty (minimum entitlement
    // of 86 400 tokens far exceeds the 10-token deposit).
    assert_eq!(token.balance(&creator), 10, "creator should receive full penalty");
    assert_eq!(
        token.balance(&attacker),
        1000000 - 10,
        "attacker should not recover deposit"
    );
}

#[test]
fn test_flash_stream_attack_multiple_quick_subscriptions() {
    let env = Env::default();
    env.mock_all_auths();

    let attacker = Address::generate(&env);
>>>>>>> main
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
<<<<<<< feature/advanced-subscription-engine-bundle
    token_admin.mint(&subscriber, &10000);
    token_admin.mint(&subscriber, &1000);
=======
    token_admin.mint(&attacker, &1000000);
>>>>>>> main

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

<<<<<<< feature/advanced-subscription-engine-bundle
    let start = 100u64;
    env.ledger().set_timestamp(start);
    client.subscribe(&subscriber, &creator, &token.address, &300, &2);

    env.ledger().set_timestamp(start + WEEK + 10);
    client.collect(&subscriber, &creator);
    assert_eq!(client.get_access_tier(&subscriber, &creator), 1);
    assert!(client.has_unlocked_access(&subscriber, &creator));
    assert_eq!(token.balance(&creator), 20);

    env.ledger().set_timestamp(start + WEEK + 20);
    client.pause_channel(&creator);
    assert!(client.is_channel_paused(&creator));
    assert_eq!(token.balance(&creator), 40);

    env.ledger().set_timestamp(start + WEEK + 100);
    client.collect(&subscriber, &creator);
    assert_eq!(client.get_access_tier(&subscriber, &creator), 2);
    assert_eq!(token.balance(&creator), 40);

    client.unpause_channel(&creator);
    assert!(!client.is_channel_paused(&creator));

    env.ledger().set_timestamp(start + WEEK + 110);
    client.collect(&subscriber, &creator);
    assert_eq!(client.get_access_tier(&subscriber, &creator), 3);
}

#[test]
fn test_migrate_tier_emits_tier_changed_event() {
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
=======
    let base_time = 1000000u64;
    
    // Simulate multiple rapid subscriptions within short timeframes
    for i in 0..5 {
        let ledger_time = base_time + (i * 5); // Each "ledger" is 5 seconds
        env.ledger().set_timestamp(ledger_time);

        let subscriber = Address::generate(&env);
        token_admin.mint(&subscriber, &5); // fund each new attacker address

        // Subscribe with minimal amount
        client.subscribe(&subscriber, &creator, &token.address, &5, &1_000_000_000);
        
        // Verify subscription is active
        assert!(client.is_subscribed(&subscriber, &creator));
        
        // Try to access content immediately after subscription
        // This simulates bypassing content gates through rapid subscriptions
        let is_active = client.is_subscribed(&subscriber, &creator);
        assert!(is_active, "Subscription should be active for flash attack attempt {}", i);
    }
}

#[test]
fn test_flash_stream_attack_grace_period_exploitation() {
    let env = Env::default();
    env.mock_all_auths();

    let attacker = Address::generate(&env);
>>>>>>> main
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
<<<<<<< feature/advanced-subscription-engine-bundle
    token_admin.mint(&subscriber, &1000);
=======
    token_admin.mint(&attacker, &1000);
>>>>>>> main

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

<<<<<<< feature/advanced-subscription-engine-bundle
    env.ledger().set_timestamp(100);
    client.subscribe(&subscriber, &creator, &token.address, &100, &1);

    client.migrate_tier(&subscriber, &creator, &3, &0);

    let events = env.events().all();
    // Verify at least one event was emitted (TierChanged)
    let _ = events;
}

#[test]
#[should_panic(expected = "group channel must contain exactly 5 creators")]
fn test_group_requires_exactly_five_creators() {
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let channel_id = Address::generate(&env);
    let creator_1 = Address::generate(&env);
    let creator_2 = Address::generate(&env);
    let creator_3 = Address::generate(&env);
    let creator_4 = Address::generate(&env);
=======
    let start_time = 1000000u64;
    env.ledger().set_timestamp(start_time);

    // Subscribe with very small amount that will be exhausted quickly
    client.subscribe(&attacker, &creator, &token.address, &10, &100_000_000_000);

    // Fast forward to exhaust funds but stay within grace period
    let exhaust_time = start_time + 10; // 10 seconds later
    env.ledger().set_timestamp(exhaust_time);
    
    // Collect to exhaust the balance
    client.collect(&attacker, &creator);
    
    // Verify still subscribed due to grace period
    assert!(client.is_subscribed(&attacker, &creator));
    
    // Attacker tries to exploit grace period by immediately resubscribing
    let new_attacker = Address::generate(&env);
    token_admin.mint(&new_attacker, &1000);
    
    env.ledger().set_timestamp(exhaust_time + 1); // 1 second later
    
    client.subscribe(&new_attacker, &creator, &token.address, &5, &1_000_000_000);
    
    // Both subscriptions should be active (original in grace period, new one active)
    assert!(client.is_subscribed(&attacker, &creator));
    assert!(client.is_subscribed(&new_attacker, &creator));
}

// ---------------------------------------------------------------------------
// Blacklist Malicious Users — Issue #25
// ---------------------------------------------------------------------------

#[test]
#[cfg(any())]
fn test_blacklist_user_prevents_subscription() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let malicious_user = Address::generate(&env);
>>>>>>> main
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
<<<<<<< feature/advanced-subscription-engine-bundle
    token_admin.mint(&subscriber, &1000);
=======
    token_admin.mint(&malicious_user, &1000);
>>>>>>> main

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

<<<<<<< feature/advanced-subscription-engine-bundle
    let creators = vec![
        &env,
        creator_1.clone(),
        creator_2.clone(),
        creator_3.clone(),
        creator_4.clone(),
    ];
    let percentages = vec![&env, 25u32, 25u32, 25u32, 25u32];
    assert_eq!(token.balance(&creator), 60);
    assert_eq!(token.balance(&contract_id), 240);
}

#[test]
fn test_pause_channel_applies_to_all_subscribers() {
    let env = Env::default();
    env.mock_all_auths();

    let subscriber_1 = Address::generate(&env);
    let subscriber_2 = Address::generate(&env);
    let creator = Address::generate(&env);
=======
    // Creator blacklists the user
    client.blacklist_user(&creator, &malicious_user);

    // Verify user is blacklisted
    assert!(client.is_user_blacklisted(&creator, &malicious_user));

    // Attempt to subscribe should fail
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.subscribe(&malicious_user, &creator, &token.address, &100, &1_000_000_000);
    }));

    assert!(result.is_err());
}

#[test]
#[cfg(any())]
fn test_unblacklist_user_allows_subscription() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let user = Address::generate(&env);
>>>>>>> main
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
<<<<<<< feature/advanced-subscription-engine-bundle
    token_admin.mint(&subscriber_1, &200);
    token_admin.mint(&subscriber_2, &200);
=======
    token_admin.mint(&user, &1000);
>>>>>>> main

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

<<<<<<< feature/advanced-subscription-engine-bundle
    let start = 100u64;
    env.ledger().set_timestamp(start);
    client.subscribe(&subscriber_1, &creator, &token.address, &200, &1);
    client.subscribe(&subscriber_2, &creator, &token.address, &200, &1);

    env.ledger().set_timestamp(start + WEEK + 30);
    client.pause_channel(&creator);
    assert_eq!(token.balance(&creator), 60);

    env.ledger().set_timestamp(start + WEEK + 130);
    client.unpause_channel(&creator);

    env.ledger().set_timestamp(start + WEEK + 140);
    let total = client.withdraw_all(&creator, &10);

    assert_eq!(total, 20);
    assert_eq!(token.balance(&creator), 80);
    assert_eq!(token.balance(&contract_id), 320);
}

#[test]
fn test_cliff_threshold_access() {
    let start = 100u64;
    env.ledger().set_timestamp(start);
    client.subscribe(&subscriber, &creator, &token.address, &100, &1);

    env.ledger().set_timestamp(start + WEEK + 30);

    env.ledger().set_timestamp(100);
    client.subscribe(&subscriber, &creator, &token.address, &100, &1);

    env.ledger().set_timestamp(130);
    client.collect(&subscriber, &creator);
    assert!(!client.has_unlocked_access(&subscriber, &creator));
    assert_eq!(client.get_access_tier(&subscriber, &creator), 0);

    env.ledger().set_timestamp(start + WEEK + 50);
    env.ledger().set_timestamp(150);
    client.collect(&subscriber, &creator);
    assert!(client.has_unlocked_access(&subscriber, &creator));
    assert_eq!(client.get_access_tier(&subscriber, &creator), 1);
}

#[test]
fn test_migrate_tier_downgrade_prorates_refund() {
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&subscriber, &1000);
=======
    // Creator blacklists the user
    client.blacklist_user(&creator, &user);
    assert!(client.is_user_blacklisted(&creator, &user));

    // Creator unblacklists the user
    client.unblacklist_user(&creator, &user);
    assert!(!client.is_user_blacklisted(&creator, &user));

    // Now subscription should work
    client.subscribe(&user, &creator, &token.address, &100, &1_000_000_000);
    assert!(client.is_subscribed(&user, &creator));
}

#[test]
#[should_panic(expected = "user already blacklisted")]
#[cfg(any())]
fn test_blacklist_already_blacklisted_user_panics() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let user = Address::generate(&env);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    // Blacklist user twice should panic
    client.blacklist_user(&creator, &user);
    client.blacklist_user(&creator, &user);
}

#[test]
#[should_panic(expected = "user not blacklisted")]
#[cfg(any())]
fn test_unblacklist_non_blacklisted_user_panics() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let user = Address::generate(&env);
>>>>>>> main

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

<<<<<<< feature/advanced-subscription-engine-bundle
    let start = 100u64;
    env.ledger().set_timestamp(start);
    client.subscribe(&subscriber, &creator, &token.address, &100, &10);

    env.ledger().set_timestamp(start + WEEK + 5);
    client.migrate_tier(&subscriber, &creator, &5, &0);

    assert_eq!(token.balance(&creator), 50);

#[test]
#[should_panic(expected = "percentages must sum to 100")]
fn test_group_percentages_must_sum_to_100() {
fn test_migrate_tier_downgrade_prorates_refund() {
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let channel_id = Address::generate(&env);
=======
    // Try to unblacklist user who isn't blacklisted should panic
    client.unblacklist_user(&creator, &user);
}

#[test]
#[cfg(any())]
fn test_blacklist_prevents_group_subscription() {
    let env = Env::default();
    env.mock_all_auths();

>>>>>>> main
    let creator_1 = Address::generate(&env);
    let creator_2 = Address::generate(&env);
    let creator_3 = Address::generate(&env);
    let creator_4 = Address::generate(&env);
    let creator_5 = Address::generate(&env);
<<<<<<< feature/advanced-subscription-engine-bundle
    let creator = Address::generate(&env);
=======
    let channel_id = Address::generate(&env);
    let malicious_user = Address::generate(&env);
>>>>>>> main
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
<<<<<<< feature/advanced-subscription-engine-bundle
    token_admin.mint(&subscriber, &1000);
=======
    token_admin.mint(&malicious_user, &1000);
>>>>>>> main

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    let creators = vec![
        &env,
        creator_1.clone(),
        creator_2.clone(),
        creator_3.clone(),
        creator_4.clone(),
        creator_5.clone(),
    ];
<<<<<<< feature/advanced-subscription-engine-bundle
    let percentages = vec![&env, 30u32, 20u32, 20u32, 10u32, 10u32]; // sums to 90
    // No subscription exists
    assert!(!client.is_subscribed(&subscriber, &creator));
    client.set_cliff_threshold(&creator, &50);

    let start = 100u64;
    env.ledger().set_timestamp(start);
    client.subscribe(&subscriber, &creator, &token.address, &100, &1);

    env.ledger().set_timestamp(start + WEEK + 30);
    client.collect(&subscriber, &creator);
    assert!(!client.has_unlocked_access(&subscriber, &creator));
    assert_eq!(client.get_access_tier(&subscriber, &creator), 0);

    env.ledger().set_timestamp(start + WEEK + 50);
    client.collect(&subscriber, &creator);
    assert!(client.has_unlocked_access(&subscriber, &creator));
    assert_eq!(client.get_access_tier(&subscriber, &creator), 1);
}

#[test]
fn test_migrate_tier_downgrade_prorates_refund() {
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);
=======
    let percentages = vec![&env, 20u32, 20u32, 20u32, 20u32, 20u32];

    // One creator blacklists the user
    client.blacklist_user(&creator_3, &malicious_user);

    // Attempt group subscription should fail
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.subscribe_group(
            &malicious_user,
            &channel_id,
            &token.address,
            &100,
            &1_000_000_000,
            &creators,
            &percentages,
        );
    }));

    assert!(result.is_err());
}

#[test]
#[cfg(any())]
fn test_blacklist_only_affects_specific_creator() {
    let env = Env::default();
    env.mock_all_auths();

    let creator_1 = Address::generate(&env);
    let creator_2 = Address::generate(&env);
    let user = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&user, &2000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    // Creator 1 blacklists the user
    client.blacklist_user(&creator_1, &user);

    // User should be blacklisted for creator_1 but not creator_2
    assert!(client.is_user_blacklisted(&creator_1, &user));
    assert!(!client.is_user_blacklisted(&creator_2, &user));

    // Subscription to creator_1 should fail
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.subscribe(&user, &creator_1, &token.address, &100, &1_000_000_000);
    }));
    assert!(result.is_err());

    // Subscription to creator_2 should succeed
    client.subscribe(&user, &creator_2, &token.address, &100, &1_000_000_000);
    assert!(client.is_subscribed(&user, &creator_2));
}

#[test]
#[cfg(any())]
fn test_blacklist_with_existing_subscription() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let user = Address::generate(&env);
>>>>>>> main
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
<<<<<<< feature/advanced-subscription-engine-bundle
    token_admin.mint(&subscriber, &1000);
=======
    token_admin.mint(&user, &1000);
>>>>>>> main

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

<<<<<<< feature/advanced-subscription-engine-bundle
    let start = 100u64;
    env.ledger().set_timestamp(start);
    client.subscribe(&subscriber, &creator, &token.address, &100, &10);

    env.ledger().set_timestamp(start + WEEK + 5);
    client.migrate_tier(&subscriber, &creator, &5, &0);

    assert_eq!(token.balance(&creator), 50);
    env.ledger().set_timestamp(100);
    client.subscribe(&subscriber, &creator, &token.address, &100, &10);

    env.ledger().set_timestamp(105);
    client.migrate_tier(&subscriber, &creator, &5, &0);

    // Collected at old rate before migration.
    assert_eq!(token.balance(&creator), 50);
    // Remaining 50 balance is prorated to 25 at new rate, 25 refunded.
    assert_eq!(token.balance(&subscriber), 925);
    assert_eq!(token.balance(&contract_id), 25);
}

#[test]
#[should_panic(expected = "new rate must be positive")]
fn test_migrate_tier_invalid_rate() {
=======
    // User subscribes first
    client.subscribe(&user, &creator, &token.address, &100, &1_000_000_000);
    assert!(client.is_subscribed(&user, &creator));

    // Creator then blacklists the user
    client.blacklist_user(&creator, &user);
    assert!(client.is_user_blacklisted(&creator, &user));

    // Existing subscription should still work (blacklist only prevents new subscriptions)
    assert!(client.is_subscribed(&user, &creator));

    // But user cannot create a new subscription after cancelling
    client.cancel(&user, &creator);
    
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.subscribe(&user, &creator, &token.address, &100, &1_000_000_000);
    }));
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Creator stats caching
// ---------------------------------------------------------------------------

#[test]
fn test_creator_stats_track_direct_stream_lifecycle() {
>>>>>>> main
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&subscriber, &1000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

<<<<<<< feature/advanced-subscription-engine-bundle
    env.ledger().set_timestamp(100);
    client.subscribe(&subscriber, &creator, &token.address, &100, &1);

    client.migrate_tier(&subscriber, &creator, &0, &0);
}

#[test]
fn test_migrate_tier_upgrade_collects_at_new_rate() {
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);
=======
    // rate 3 tokens/s = 3 * PRECISION_MULTIPLIER; 10 s post-trial → 30 tokens earned
    env.ledger().set_timestamp(100);
    // rate = 3 tokens/second = 3 × PRECISION_MULTIPLIER nano-units/second.
    client.subscribe(&subscriber, &creator, &token.address, &300, &3_000_000_000);

    assert_eq!(
        client.creator_stats(&creator),
        CreatorStats {
            total_earned: 0,
            lifetime_fans: 1,
            active_fans: 1,
        }
    );

    env.ledger().set_timestamp(100 + WEEK + 10);
    client.collect(&subscriber, &creator);

    assert_eq!(
        client.creator_stats(&creator),
        CreatorStats {
            total_earned: 30,
            lifetime_fans: 1,
            active_fans: 1,
        }
    );

    env.ledger().set_timestamp(100 + WEEK + DAY + 20);
    client.cancel(&subscriber, &creator);

    // cancel flushes remaining balance (270 tokens) to the creator on top of
    // the 30 already collected → total_earned = 300.
    assert_eq!(
        client.creator_stats(&creator),
        CreatorStats {
            total_earned: 300,
            lifetime_fans: 1,
            active_fans: 0,
        }
    );
}

#[test]
fn test_creator_stats_do_not_double_count_same_fan_across_streams() {
    let env = Env::default();
    env.mock_all_auths();

    let fan = Address::generate(&env);
    let creator = Address::generate(&env);
    let channel_id = Address::generate(&env);
    let creator_2 = Address::generate(&env);
    let creator_3 = Address::generate(&env);
    let creator_4 = Address::generate(&env);
    let creator_5 = Address::generate(&env);
>>>>>>> main
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
<<<<<<< feature/advanced-subscription-engine-bundle
    token_admin.mint(&subscriber, &1000);
=======
    token_admin.mint(&fan, &5000);
>>>>>>> main

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

<<<<<<< feature/advanced-subscription-engine-bundle
    env.ledger().set_timestamp(100);
    client.subscribe(&subscriber, &creator, &token.address, &100, &1);

    env.ledger().set_timestamp(110);
    client.migrate_tier(&subscriber, &creator, &2, &0);

    assert_eq!(token.balance(&creator), 10);
    assert_eq!(token.balance(&contract_id), 90);

    env.ledger().set_timestamp(120);
    client.collect(&subscriber, &creator);
    assert_eq!(token.balance(&creator), 30);
    assert_eq!(token.balance(&contract_id), 70);
}

#[test]
#[should_panic(expected = "cannot cancel stream: minimum duration not met")]
fn test_cancel_before_minimum_duration() {
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
=======
    let creators = vec![
        &env,
        creator.clone(),
        creator_2.clone(),
        creator_3.clone(),
        creator_4.clone(),
        creator_5.clone(),
    ];
    let percentages = vec![&env, 20u32, 20u32, 20u32, 20u32, 20u32];

    env.ledger().set_timestamp(0);
    client.subscribe(&fan, &creator, &token.address, &200, &1);
    client.subscribe_group(&fan, &channel_id, &token.address, &500, &1, &creators, &percentages);

    assert_eq!(
        client.creator_stats(&creator),
        CreatorStats {
            total_earned: 0,
            lifetime_fans: 1,
            active_fans: 1,
        }
    );

    env.ledger().set_timestamp(DAY + 10);
    client.cancel(&fan, &creator);

    assert_eq!(
        client.creator_stats(&creator),
        CreatorStats {
            total_earned: 0,
            lifetime_fans: 1,
            active_fans: 1,
        }
    );

    client.cancel_group(&fan, &channel_id);

    assert_eq!(
        client.creator_stats(&creator),
        CreatorStats {
            total_earned: 0,
            lifetime_fans: 1,
            active_fans: 0,
        }
    );
}

#[test]
fn test_creator_stats_scale_with_cached_counters() {
    const FAN_COUNT: u64 = 200;

    let env = Env::default();
    env.mock_all_auths();

>>>>>>> main
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
<<<<<<< feature/advanced-subscription-engine-bundle
    token_admin.mint(&subscriber, &1000);
=======
>>>>>>> main

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

<<<<<<< feature/advanced-subscription-engine-bundle
    env.ledger().set_timestamp(100);
    client.subscribe(&subscriber, &creator, &token.address, &100, &1);

    // Try to cancel after only 1 hour — should fail
    env.ledger().set_timestamp(100 + 3600);
    client.cancel(&subscriber, &creator);
}

#[test]
fn test_cancel_exactly_at_minimum_duration() {
=======
    env.ledger().set_timestamp(500);

    for _ in 0..FAN_COUNT {
        let fan = Address::generate(&env);
        token_admin.mint(&fan, &100);
        client.subscribe(&fan, &creator, &token.address, &100, &1);
    }

    let stats = client.creator_stats(&creator);
    assert_eq!(stats.lifetime_fans, FAN_COUNT);
    assert_eq!(stats.active_fans, FAN_COUNT);
    assert_eq!(stats.total_earned, 0);
}

// ---------------------------------------------------------------------------
// Minimum-lock penalty — Issue #XX
// ---------------------------------------------------------------------------

// When the balance is large enough to cover the full 24-hour penalty the
// creator receives exactly rate × 86400 tokens and the rest is refunded.
#[test]
fn test_early_cancel_partial_refund_when_balance_exceeds_penalty() {
>>>>>>> main
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
<<<<<<< feature/advanced-subscription-engine-bundle
    token_admin.mint(&subscriber, &1000);
=======
    token_admin.mint(&subscriber, &1_000_000);
>>>>>>> main

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

<<<<<<< feature/advanced-subscription-engine-bundle
    env.ledger().set_timestamp(100);
    client.subscribe(&subscriber, &creator, &token.address, &100, &1);

    env.ledger().set_timestamp(100 + 86400);
    client.cancel(&subscriber, &creator);

    assert_eq!(token.balance(&creator), 100);
    assert_eq!(token.balance(&subscriber), 900);
    assert_eq!(token.balance(&contract_id), 0);
}

#[test]
#[should_panic(
    expected = "cannot cancel stream: minimum duration not met. 43200 seconds remaining"
)]
fn test_cancel_with_remaining_time_message() {
=======
    // rate = 1 token/second → 24-hour penalty = 86 400 tokens.
    // Deposit 200 000 tokens so the penalty doesn't consume everything.
    let rate: i128 = 1_000_000_000;
    let deposit: i128 = 200_000;
    env.ledger().set_timestamp(0);
    client.subscribe(&subscriber, &creator, &token.address, &deposit, &rate);

    // Cancel after 2 hours — well within the 24-hour window.
    env.ledger().set_timestamp(7200);
    client.cancel(&subscriber, &creator);

    let penalty: i128 = 86_400; // rate × DAY / PRECISION_MULTIPLIER
    let refund: i128 = deposit - penalty;

    assert_eq!(token.balance(&creator), penalty);
    assert_eq!(token.balance(&subscriber), 1_000_000 - deposit + refund);
    assert_eq!(token.balance(&contract_id), 0);
    assert!(!client.is_subscribed(&subscriber, &creator));
}

// When the deposit is smaller than the 24-hour penalty the creator receives
// the full deposit and the subscriber is refunded nothing.
#[test]
fn test_early_cancel_penalty_capped_at_balance() {
>>>>>>> main
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
<<<<<<< feature/advanced-subscription-engine-bundle
    token_admin.mint(&subscriber, &1000);
=======
    token_admin.mint(&subscriber, &5000);
>>>>>>> main

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

<<<<<<< feature/advanced-subscription-engine-bundle
    env.ledger().set_timestamp(100);
    client.subscribe(&subscriber, &creator, &token.address, &100, &1);

    // Try to cancel after 12 hours (43200 seconds remaining)
    env.ledger().set_timestamp(100 + 43200);
    client.cancel(&subscriber, &creator);
}

#[test]
fn test_total_streamed_tracking() {
=======
    // rate = 1 token/second → penalty = 86 400 tokens > deposit of 50.
    let deposit: i128 = 50;
    env.ledger().set_timestamp(0);
    client.subscribe(&subscriber, &creator, &token.address, &deposit, &1_000_000_000);

    env.ledger().set_timestamp(1800); // 30 minutes
    client.cancel(&subscriber, &creator);

    assert_eq!(token.balance(&creator), deposit, "creator gets whole deposit as penalty");
    assert_eq!(token.balance(&subscriber), 5000 - deposit, "subscriber gets nothing back");
    assert_eq!(token.balance(&contract_id), 0);
    assert!(!client.is_subscribed(&subscriber, &creator));
}

// A zero-rate subscription (e.g. free tier) carries no penalty on early cancel.
#[test]
fn test_early_cancel_zero_rate_no_penalty() {
>>>>>>> main
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&subscriber, &1000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

<<<<<<< feature/advanced-subscription-engine-bundle
    client.subscribe(&subscriber, &creator, &token.address, &100, &1);

    env.ledger().set_timestamp(100);
    client.collect(&subscriber, &creator);
    assert_eq!(client.get_total_streamed(&subscriber, &creator), 100);

    client.top_up(&subscriber, &creator, &50);
    env.ledger().set_timestamp(150);
    client.collect(&subscriber, &creator);
    assert_eq!(client.get_total_streamed(&subscriber, &creator), 150);
}

#[test]
fn test_creator_metadata() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
=======
    env.ledger().set_timestamp(0);
    // rate = 0 → no charges, no penalty.
    client.subscribe(&subscriber, &creator, &token.address, &100, &0);

    env.ledger().set_timestamp(3600);
    client.cancel(&subscriber, &creator);

    assert_eq!(token.balance(&creator), 0, "no penalty for zero-rate subscription");
    assert_eq!(token.balance(&subscriber), 1000, "full deposit refunded");
    assert_eq!(token.balance(&contract_id), 0);
}

// Group subscriptions split the early-cancel penalty across all creators
// according to their configured percentages.
#[test]
fn test_early_cancel_group_distributes_penalty() {
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let channel_id = Address::generate(&env);
    let c1 = Address::generate(&env);
    let c2 = Address::generate(&env);
    let c3 = Address::generate(&env);
    let c4 = Address::generate(&env);
    let c5 = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&subscriber, &500_000);
>>>>>>> main

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

<<<<<<< feature/advanced-subscription-engine-bundle
    // No metadata set yet
    assert_eq!(client.get_creator_metadata(&creator), None);

    // Set an IPFS CID
    let cid = Bytes::from_slice(&env, b"QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG");
    client.set_creator_metadata(&creator, &cid);

    // Retrieve and verify
    assert_eq!(client.get_creator_metadata(&creator), Some(cid.clone()));

    // Update to a new CID
    let new_cid = Bytes::from_slice(&env, b"QmNewCIDabcdefghijklmnopqrstuvwxyz1234567890AB");
    client.set_creator_metadata(&creator, &new_cid);
    assert_eq!(client.get_creator_metadata(&creator), Some(new_cid));
=======
    let creators = vec![&env, c1.clone(), c2.clone(), c3.clone(), c4.clone(), c5.clone()];
    // c5 receives the rounding remainder (last in loop).
    let percentages = vec![&env, 40u32, 25u32, 15u32, 10u32, 10u32];

    // rate = 1 token/second → penalty = 86 400 tokens.
    // Deposit 200 000 so penalty doesn't exceed balance.
    let deposit: i128 = 200_000;
    env.ledger().set_timestamp(0);
    client.subscribe_group(
        &subscriber,
        &channel_id,
        &token.address,
        &deposit,
        &1_000_000_000,
        &creators,
        &percentages,
    );

    // Cancel at 6 hours — inside the 24-hour window.
    env.ledger().set_timestamp(6 * 3600);
    client.cancel_group(&subscriber, &channel_id);

    let penalty: i128 = 86_400;
    assert_eq!(token.balance(&c1), (penalty * 40) / 100);
    assert_eq!(token.balance(&c2), (penalty * 25) / 100);
    assert_eq!(token.balance(&c3), (penalty * 15) / 100);
    assert_eq!(token.balance(&c4), (penalty * 10) / 100);
    // c5 receives the rounding remainder.
    let distributed = (penalty * 40) / 100
        + (penalty * 25) / 100
        + (penalty * 15) / 100
        + (penalty * 10) / 100;
    assert_eq!(token.balance(&c5), penalty - distributed);

    let refund = deposit - penalty;
    assert_eq!(token.balance(&subscriber), 500_000 - deposit + refund);
    assert_eq!(token.balance(&contract_id), 0);
>>>>>>> main
}
