#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger},
    token, Address, Env,
};

const DAY: u64 = 24 * 60 * 60;
const WEEK: u64 = 7 * DAY;

fn setup() -> (
    Env,
    SubStreamContractClient<'static>,
    token::Client<'static>,
    Address, // fan
    Address, // creator
) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token = token::Client::new(&env, &sac.address());
    let token_admin = token::StellarAssetClient::new(&env, &sac.address());

    let fan = Address::generate(&env);
    let creator = Address::generate(&env);
    token_admin.mint(&fan, &1_000_000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    let client: SubStreamContractClient<'static> = unsafe { core::mem::transmute(client) };
    let token: token::Client<'static> = unsafe { core::mem::transmute(token) };

    (env, client, token, fan, creator)
}

// ---------------------------------------------------------------------------
// set_cliff_threshold — basic
// ---------------------------------------------------------------------------

#[test]
fn test_set_cliff_threshold_stores_value() {
    let (env, client, _token, _fan, creator) = setup();
    client.set_cliff_threshold(&creator, &500);
    // No panic = stored. Verified via check_cliff_access below.
    let _ = env; // suppress unused warning
}

#[test]
#[should_panic(expected = "threshold must be positive")]
fn test_set_cliff_threshold_zero_panics() {
    let (_env, client, _token, _fan, creator) = setup();
    client.set_cliff_threshold(&creator, &0);
}

// ---------------------------------------------------------------------------
// check_cliff_access — no threshold set → always false
// ---------------------------------------------------------------------------

#[test]
fn test_check_cliff_access_no_threshold_returns_false() {
    let (_env, client, _token, fan, creator) = setup();
    assert!(!client.check_cliff_access(&fan, &creator));
}

// ---------------------------------------------------------------------------
// get_total_contributed — zero before any payment
// ---------------------------------------------------------------------------

#[test]
fn test_get_total_contributed_zero_initially() {
    let (_env, client, _token, fan, creator) = setup();
    assert_eq!(client.get_total_contributed(&fan, &creator), 0);
}

// ---------------------------------------------------------------------------
// Tip credits contribution and unlocks cliff
// ---------------------------------------------------------------------------

#[test]
fn test_tip_credits_contribution() {
    let (env, client, token, fan, creator) = setup();
    env.ledger().set_timestamp(0);

    client.tip(&fan, &creator, &token.address, &200);
    assert_eq!(client.get_total_contributed(&fan, &creator), 200);
}

#[test]
fn test_cliff_not_unlocked_below_threshold() {
    let (env, client, token, fan, creator) = setup();
    env.ledger().set_timestamp(0);

    client.set_cliff_threshold(&creator, &500);
    client.tip(&fan, &creator, &token.address, &300);

    assert!(!client.check_cliff_access(&fan, &creator));
}

#[test]
fn test_cliff_unlocked_at_threshold() {
    let (env, client, token, fan, creator) = setup();
    env.ledger().set_timestamp(0);

    client.set_cliff_threshold(&creator, &500);
    client.tip(&fan, &creator, &token.address, &500);

    assert!(client.check_cliff_access(&fan, &creator));
}

#[test]
fn test_cliff_unlocked_above_threshold() {
    let (env, client, token, fan, creator) = setup();
    env.ledger().set_timestamp(0);

    client.set_cliff_threshold(&creator, &100);
    client.tip(&fan, &creator, &token.address, &300);

    assert!(client.check_cliff_access(&fan, &creator));
}

// ---------------------------------------------------------------------------
// CliffUnlocked event emitted exactly once
// ---------------------------------------------------------------------------

#[test]
fn test_cliff_unlocked_event_emitted_on_crossing() {
    let (env, client, token, fan, creator) = setup();
    env.ledger().set_timestamp(0);

    client.set_cliff_threshold(&creator, &500);

    // First tip: below threshold — transfer + TipReceived (2 events)
    client.tip(&fan, &creator, &token.address, &300);
    let events_below = env.events().all().events().len();

    // Second tip: crosses threshold — transfer + TipReceived + CliffUnlocked (3 events)
    client.tip(&fan, &creator, &token.address, &200);
    let events_crossing = env.events().all().events().len();

    assert_eq!(events_below, 2, "transfer + TipReceived before crossing");
    assert_eq!(
        events_crossing, 3,
        "transfer + TipReceived + CliffUnlocked on crossing"
    );
}

#[test]
fn test_cliff_unlocked_event_not_emitted_again_after_crossing() {
    let (env, client, token, fan, creator) = setup();
    env.ledger().set_timestamp(0);

    client.set_cliff_threshold(&creator, &100);

    // Cross the threshold
    client.tip(&fan, &creator, &token.address, &200);

    // Subsequent tip — transfer + TipReceived only, no CliffUnlocked (2 events)
    client.tip(&fan, &creator, &token.address, &100);
    let events = env.events().all().events().len();
    assert_eq!(
        events, 2,
        "CliffUnlocked must not fire again after threshold already crossed"
    );
}

// ---------------------------------------------------------------------------
// Streaming subscription credits contribution
// ---------------------------------------------------------------------------

#[test]
fn test_subscription_stream_credits_contribution() {
    let (env, client, token, fan, creator) = setup();

    env.ledger().set_timestamp(0);
    // rate = 1 token/sec; deposit 10_000 tokens
    client.subscribe(
        &fan,
        &creator,
        &token.address,
        &10_000,
        &1_000_000_000,
        &None,
    );

    // Advance past trial + 100 paid seconds
    env.ledger().set_timestamp(WEEK + 100);
    client.collect(&fan, &creator);

    // Fan should have 100 tokens credited
    assert_eq!(client.get_total_contributed(&fan, &creator), 100);
}

#[test]
fn test_subscription_stream_unlocks_cliff() {
    let (env, client, token, fan, creator) = setup();

    client.set_cliff_threshold(&creator, &50);

    env.ledger().set_timestamp(0);
    client.subscribe(
        &fan,
        &creator,
        &token.address,
        &10_000,
        &1_000_000_000,
        &None,
    );

    // Advance past trial + 60 paid seconds → 60 tokens streamed
    env.ledger().set_timestamp(WEEK + 60);
    client.collect(&fan, &creator);

    assert!(client.check_cliff_access(&fan, &creator));
}

// ---------------------------------------------------------------------------
// Contributions are per-creator (fan A's total for creator X ≠ creator Y)
// ---------------------------------------------------------------------------

#[test]
fn test_contributions_are_per_creator() {
    let (env, client, token, fan, creator) = setup();
    let creator2 = Address::generate(&env);
    env.ledger().set_timestamp(0);

    client.set_cliff_threshold(&creator, &500);
    client.set_cliff_threshold(&creator2, &500);

    client.tip(&fan, &creator, &token.address, &500);

    assert!(client.check_cliff_access(&fan, &creator));
    assert!(!client.check_cliff_access(&fan, &creator2));
}

// ---------------------------------------------------------------------------
// Cumulative tips across multiple transactions
// ---------------------------------------------------------------------------

#[test]
fn test_cumulative_tips_accumulate() {
    let (env, client, token, fan, creator) = setup();
    env.ledger().set_timestamp(0);

    client.set_cliff_threshold(&creator, &300);

    client.tip(&fan, &creator, &token.address, &100);
    client.tip(&fan, &creator, &token.address, &100);
    assert!(!client.check_cliff_access(&fan, &creator));

    client.tip(&fan, &creator, &token.address, &100);
    assert!(client.check_cliff_access(&fan, &creator));
    assert_eq!(client.get_total_contributed(&fan, &creator), 300);
}
