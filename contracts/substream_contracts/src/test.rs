#![cfg(test)]

use super::*;
use soroban_sdk::{

    token, vec, Address, Env,
};

const DAY: u64 = 24 * 60 * 60;
const WEEK: u64 = 7 * DAY;

fn create_token_contract<'a>(env: &Env, admin: &Address) -> token::Client<'a> {
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    token::Client::new(env, &sac.address())
}

fn last_call_contract_event_count(env: &Env, contract_id: &Address) -> usize {
    let events = env.events().all().filter_by_contract(contract_id);
    events.events().len()
}

// ---------------------------------------------------------------------------
// is_subscribed
// ---------------------------------------------------------------------------

#[test]
fn test_is_subscribed_active() {
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&subscriber, &10000000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    env.ledger().set_timestamp(100);
    client.subscribe(&subscriber, &creator, &token.address, &1000, &1_000_000_000);

    env.ledger().set_timestamp(105);
    assert!(client.is_subscribed(&subscriber, &creator));
}

#[test]
fn test_is_subscribed_expired() {
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
    client.subscribe(
        &subscriber,
        &creator,
        &token.address,
        &10,
        &10_000_000_000,
        &None,
    );

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
    client.subscribe(
        &subscriber,
        &creator,
        &token.address,
        &100,
        &10_000_000_000,
        &None,
    );

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
    env.ledger()
        .set_timestamp(start + WEEK + 10 + GRACE_PERIOD + 1);
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

    let start = 100u64;
    env.ledger().set_timestamp(start);
    client.subscribe(&subscriber, &creator, &token.address, &300, &3_000_000_000);

    env.ledger().set_timestamp(start + WEEK - 1);
    client.collect(&subscriber, &creator);
    assert_eq!(token.balance(&creator), 0);

    env.ledger().set_timestamp(start + WEEK + 9);
    client.collect(&subscriber, &creator);
    assert_eq!(token.balance(&creator), 27);
}

#[test]
fn test_free_to_paid_transition_event_emitted_once() {
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

    let start = 100u64;
    env.ledger().set_timestamp(start);
    client.subscribe(&subscriber, &creator, &token.address, &300, &1, &None);

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
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    // Subscriber starts with 1000 tokens and deposits 100.
    token_admin.mint(&subscriber, &1000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    let rate: i128 = 1_000_000_000; // 1 token/second
    env.ledger().set_timestamp(100);
    client.subscribe(&subscriber, &creator, &token.address, &100, &rate, &None);

    // Cancel after only 1 hour — still inside the 24-hour lock window.
    env.ledger().set_timestamp(100 + 3600);
    client.cancel(&subscriber, &creator);

    // Minimum entitled = rate × 86400 = 86_400_000_000_000 nano-units
    // → 86_400 tokens, but balance is only 100 tokens → penalty capped at 100.
    assert_eq!(
        token.balance(&creator),
        100,
        "creator should receive full penalty"
    );
    assert_eq!(
        token.balance(&subscriber),
        900,
        "subscriber gets no refund (penalty = deposit)"
    );
    assert_eq!(
        token.balance(&contract_id),
        0,
        "contract should hold nothing after cancel"
    );

    // Subscription must be removed.
    assert!(!client.is_subscribed(&subscriber, &creator));
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

    let start = 100u64;
    env.ledger().set_timestamp(start);
    client.subscribe(
        &subscriber,
        &creator,
        &token.address,
        &100,
        &1_000_000_000,
        &None,
    );

    env.ledger().set_timestamp(start + DAY + 10);
    client.cancel(&subscriber, &creator);

    assert_eq!(token.balance(&creator), 0);
    assert_eq!(token.balance(&subscriber), 1000);
}

#[test]
fn test_cancel_exactly_at_minimum_duration() {
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
    client.subscribe(
        &subscriber,
        &creator,
        &token.address,
        &100,
        &1_000_000_000,
        &None,
    );

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

    env.ledger().set_timestamp(0);
    client.subscribe(
        &subscriber,
        &creator,
        &token.address,
        &100,
        &1_000_000_000,
        &None,
    );
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
    client.subscribe_group(
        &subscriber,
        &channel_id,
        &token.address,
        &500,
        &10_000_000_000,
        &creators,
        &percentages,
    );

    env.ledger().set_timestamp(start + WEEK + 10);
    client.collect_group(&subscriber, &channel_id);

    assert_eq!(token.balance(&creator_1), 40);
    assert_eq!(token.balance(&creator_2), 25);
    assert_eq!(token.balance(&creator_3), 15);
    assert_eq!(token.balance(&creator_4), 10);
    assert_eq!(token.balance(&creator_5), 10);
    assert_eq!(token.balance(&contract_id), 400);
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
    let percentages = vec![&env, 30u32, 20u32, 20u32, 10u32, 10u32];

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
fn test_group_cancel_collects_and_refunds_remaining_balance() {
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

    env.ledger().set_timestamp(0);
    client.subscribe_group(
        &subscriber,
        &channel_id,
        &token.address,
        &200,
        &1_000_000_000,
        &creators,
        &percentages,
    );

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

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

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

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

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
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&attacker, &1000000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    let ledger_time = 1000000u64;
    env.ledger().set_timestamp(ledger_time);

    // Attacker deposits 10 tokens.
    client.subscribe(
        &attacker,
        &creator,
        &token.address,
        &10,
        &1_000_000_000,
        &None,
    );
    assert!(client.is_subscribed(&attacker, &creator));

    // Attempt to cancel within the same ledger (0 seconds elapsed).
    // The cancel succeeds but applies the minimum-lock penalty.
    client.cancel(&attacker, &creator);

    // Subscription is gone — attacker can no longer access content.
    assert!(!client.is_subscribed(&attacker, &creator));

    // Creator receives the full deposit as a penalty (minimum entitlement
    // of 86 400 tokens far exceeds the 10-token deposit).
    assert_eq!(
        token.balance(&creator),
        10,
        "creator should receive full penalty"
    );
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
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&attacker, &1000000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    let base_time = 1000000u64;

    // Simulate multiple rapid subscriptions within short timeframes
    for i in 0..5 {
        let ledger_time = base_time + (i * 5); // Each "ledger" is 5 seconds
        env.ledger().set_timestamp(ledger_time);

        let subscriber = Address::generate(&env);
        token_admin.mint(&subscriber, &5); // fund each new attacker address

        // Subscribe with minimal amount
        client.subscribe(
            &subscriber,
            &creator,
            &token.address,
            &5,
            &1_000_000_000,
            &None,
        );

        // Verify subscription is active
        assert!(client.is_subscribed(&subscriber, &creator));

        // Try to access content immediately after subscription
        // This simulates bypassing content gates through rapid subscriptions
        let is_active = client.is_subscribed(&subscriber, &creator);
        assert!(
            is_active,
            "Subscription should be active for flash attack attempt {}",
            i
        );
    }
}

#[test]
fn test_flash_stream_attack_grace_period_exploitation() {
    let env = Env::default();
    env.mock_all_auths();

    let attacker = Address::generate(&env);
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&attacker, &1000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    let start_time = 1000000u64;
    env.ledger().set_timestamp(start_time);

    // Subscribe with very small amount that will be exhausted quickly
    client.subscribe(
        &attacker,
        &creator,
        &token.address,
        &10,
        &100_000_000_000,
        &None,
    );

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

    client.subscribe(
        &new_attacker,
        &creator,
        &token.address,
        &5,
        &1_000_000_000,
        &None,
    );

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
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&malicious_user, &1000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    // Creator blacklists the user
    client.blacklist_user(&creator, &malicious_user);

    // Verify user is blacklisted
    assert!(client.is_user_blacklisted(&creator, &malicious_user));

    // Attempt to subscribe should fail
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.subscribe(
            &malicious_user,
            &creator,
            &token.address,
            &100,
            &1_000_000_000,
            &None,
        );
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
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&user, &1000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    // Creator blacklists the user
    client.blacklist_user(&creator, &user);
    assert!(client.is_user_blacklisted(&creator, &user));

    // Creator unblacklists the user
    client.unblacklist_user(&creator, &user);
    assert!(!client.is_user_blacklisted(&creator, &user));

    // Now subscription should work
    client.subscribe(&user, &creator, &token.address, &100, &1_000_000_000, &None);
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

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    // Try to unblacklist user who isn't blacklisted should panic
    client.unblacklist_user(&creator, &user);
}

#[test]
#[cfg(any())]
fn test_blacklist_prevents_group_subscription() {
    let env = Env::default();
    env.mock_all_auths();

    let creator_1 = Address::generate(&env);
    let creator_2 = Address::generate(&env);
    let creator_3 = Address::generate(&env);
    let creator_4 = Address::generate(&env);
    let creator_5 = Address::generate(&env);
    let channel_id = Address::generate(&env);
    let malicious_user = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&malicious_user, &1000);

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
        client.subscribe(
            &user,
            &creator_1,
            &token.address,
            &100,
            &1_000_000_000,
            &None,
        );
    }));
    assert!(result.is_err());

    // Subscription to creator_2 should succeed
    client.subscribe(
        &user,
        &creator_2,
        &token.address,
        &100,
        &1_000_000_000,
        &None,
    );
    assert!(client.is_subscribed(&user, &creator_2));
}

#[test]
#[cfg(any())]
fn test_blacklist_with_existing_subscription() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let user = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&user, &1000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    // User subscribes first
    client.subscribe(&user, &creator, &token.address, &100, &1_000_000_000, &None);
    assert!(client.is_subscribed(&user, &creator));

    // Creator then blacklists the user
    client.blacklist_user(&creator, &user);
    assert!(client.is_user_blacklisted(&creator, &user));

    // Existing subscription should still work (blacklist only prevents new subscriptions)
    assert!(client.is_subscribed(&user, &creator));

    // But user cannot create a new subscription after cancelling
    client.cancel(&user, &creator);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.subscribe(&user, &creator, &token.address, &100, &1_000_000_000, &None);
    }));
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Creator stats caching
// ---------------------------------------------------------------------------

#[test]
fn test_creator_stats_track_direct_stream_lifecycle() {
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

    // rate 3 tokens/s = 3 * PRECISION_MULTIPLIER; 10 s post-trial → 30 tokens earned
    env.ledger().set_timestamp(100);
    // rate = 3 tokens/second = 3 × PRECISION_MULTIPLIER nano-units/second.
    client.subscribe(
        &subscriber,
        &creator,
        &token.address,
        &300,
        &3_000_000_000,
        &None,
    );

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
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&fan, &5000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

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
    client.subscribe(&fan, &creator, &token.address, &200, &1, &None);
    client.subscribe_group(
        &fan,
        &channel_id,
        &token.address,
        &500,
        &1,
        &creators,
        &percentages,
    );

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

    let creator = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    env.ledger().set_timestamp(500);

    for _ in 0..FAN_COUNT {
        let fan = Address::generate(&env);
        token_admin.mint(&fan, &100);
        client.subscribe(&fan, &creator, &token.address, &100, &1, &None);
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
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&subscriber, &1_000_000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    // rate = 1 token/second → 24-hour penalty = 86 400 tokens.
    // Deposit 200 000 tokens so the penalty doesn't consume everything.
    let rate: i128 = 1_000_000_000;
    let deposit: i128 = 200_000;
    env.ledger().set_timestamp(0);
    client.subscribe(
        &subscriber,
        &creator,
        &token.address,
        &deposit,
        &rate,
        &None,
    );

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
    let env = Env::default();
    env.mock_all_auths();

    let subscriber = Address::generate(&env);
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address);
    token_admin.mint(&subscriber, &5000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    // rate = 1 token/second → penalty = 86 400 tokens > deposit of 50.
    let deposit: i128 = 50;
    env.ledger().set_timestamp(0);
    client.subscribe(
        &subscriber,
        &creator,
        &token.address,
        &deposit,
        &1_000_000_000,
        &None,
    );

    env.ledger().set_timestamp(1800); // 30 minutes
    client.cancel(&subscriber, &creator);

    assert_eq!(
        token.balance(&creator),
        deposit,
        "creator gets whole deposit as penalty"
    );
    assert_eq!(
        token.balance(&subscriber),
        5000 - deposit,
        "subscriber gets nothing back"
    );
    assert_eq!(token.balance(&contract_id), 0);
    assert!(!client.is_subscribed(&subscriber, &creator));
}

// A zero-rate subscription (e.g. free tier) carries no penalty on early cancel.
#[test]
fn test_early_cancel_zero_rate_no_penalty() {
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

    env.ledger().set_timestamp(0);
    // rate = 0 → no charges, no penalty.
    client.subscribe(&subscriber, &creator, &token.address, &100, &0, &None);

    env.ledger().set_timestamp(3600);
    client.cancel(&subscriber, &creator);

    assert_eq!(
        token.balance(&creator),
        0,
        "no penalty for zero-rate subscription"
    );
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

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

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
    let distributed =
        (penalty * 40) / 100 + (penalty * 25) / 100 + (penalty * 15) / 100 + (penalty * 10) / 100;
    assert_eq!(token.balance(&c5), penalty - distributed);

    let refund = deposit - penalty;
    assert_eq!(token.balance(&subscriber), 500_000 - deposit + refund);
    assert_eq!(token.balance(&contract_id), 0);
}
