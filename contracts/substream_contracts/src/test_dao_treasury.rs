#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

const DAY: u64 = 24 * 60 * 60;
const WEEK: u64 = 7 * DAY;

fn setup() -> (
    Env,
    SubStreamContractClient<'static>,
    token::Client<'static>,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token = token::Client::new(&env, &sac.address());
    let token_admin = token::StellarAssetClient::new(&env, &sac.address());

    let dao = Address::generate(&env);
    token_admin.mint(&dao, &1_000_000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    // SAFETY: lifetime extension is safe here — env owns all allocations and
    // outlives the test function.
    let client: SubStreamContractClient<'static> = unsafe { core::mem::transmute(client) };
    let token: token::Client<'static> = unsafe { core::mem::transmute(token) };

    (env, client, token, dao, admin)
}

// ---------------------------------------------------------------------------
// dao_grant — happy path
// ---------------------------------------------------------------------------

#[test]
fn test_dao_grant_initiates_stream() {
    let (env, client, token, dao, _admin) = setup();
    let creator = Address::generate(&env);

    env.ledger().set_timestamp(1000);
    client.dao_grant(&dao, &creator, &token.address, &10_000, &1_000_000_000);

    // DAO balance reduced by grant amount
    assert_eq!(token.balance(&dao), 990_000);
}

// ---------------------------------------------------------------------------
// collect_grant — no payout during free trial
// ---------------------------------------------------------------------------

#[test]
fn test_collect_grant_no_payout_during_trial() {
    let (env, client, token, dao, _admin) = setup();
    let creator = Address::generate(&env);

    env.ledger().set_timestamp(1000);
    client.dao_grant(&dao, &creator, &token.address, &10_000, &1_000_000_000);

    // Advance 3 days — still within the 7-day free trial
    env.ledger().set_timestamp(1000 + 3 * DAY);
    client.collect_grant(&dao, &creator);

    assert_eq!(token.balance(&creator), 0);
}

// ---------------------------------------------------------------------------
// collect_grant — payout after trial ends
// ---------------------------------------------------------------------------

#[test]
fn test_collect_grant_pays_after_trial() {
    let (env, client, token, dao, _admin) = setup();
    let creator = Address::generate(&env);

    env.ledger().set_timestamp(0);
    // rate = 1 token/sec (in nano units: 1_000_000_000)
    client.dao_grant(&dao, &creator, &token.address, &100_000, &1_000_000_000);

    // Advance past trial + 10 seconds of paid streaming
    env.ledger().set_timestamp(WEEK + 10);
    client.collect_grant(&dao, &creator);

    // Creator should have received 10 tokens
    assert_eq!(token.balance(&creator), 10);
}

// ---------------------------------------------------------------------------
// revoke_grant — before minimum duration panics
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "minimum duration not met")]
fn test_revoke_grant_before_minimum_duration_panics() {
    let (env, client, token, dao, _admin) = setup();
    let creator = Address::generate(&env);

    env.ledger().set_timestamp(0);
    client.dao_grant(&dao, &creator, &token.address, &10_000, &1_000_000_000);

    // Try to revoke after only 1 hour — should panic
    env.ledger().set_timestamp(3600);
    client.revoke_grant(&dao, &creator);
}

// ---------------------------------------------------------------------------
// revoke_grant — refunds unstreamed balance after minimum duration
// ---------------------------------------------------------------------------

#[test]
fn test_revoke_grant_refunds_unstreamed_balance() {
    let (env, client, token, dao, _admin) = setup();
    let creator = Address::generate(&env);

    env.ledger().set_timestamp(0);
    // rate = 1 token/sec; deposit 100_000 tokens — stream will run for ~100_000 s
    client.dao_grant(&dao, &creator, &token.address, &100_000, &1_000_000_000);

    let dao_balance_before = token.balance(&dao); // 900_000

    // Advance past minimum duration (24 h) but well before balance exhaustion
    env.ledger().set_timestamp(DAY + 1);
    client.revoke_grant(&dao, &creator);

    // DAO should have been refunded most of the deposit
    let dao_balance_after = token.balance(&dao);
    assert!(
        dao_balance_after > dao_balance_before,
        "DAO should receive a refund"
    );

    // Creator should have received the streamed portion (only post-trial seconds)
    // Trial = 7 days; revoke at 1 day + 1 sec → still in trial → creator gets 0
    assert_eq!(token.balance(&creator), 0);
}

// ---------------------------------------------------------------------------
// dao_grant — duplicate grant panics
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "grant already active")]
fn test_dao_grant_duplicate_panics() {
    let (env, client, token, dao, _admin) = setup();
    let creator = Address::generate(&env);

    env.ledger().set_timestamp(0);
    client.dao_grant(&dao, &creator, &token.address, &10_000, &1_000_000_000);
    client.dao_grant(&dao, &creator, &token.address, &10_000, &1_000_000_000);
}

// ---------------------------------------------------------------------------
// dao_grant — invalid inputs panic
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "grant amount must be positive")]
fn test_dao_grant_zero_amount_panics() {
    let (env, client, token, dao, _admin) = setup();
    let creator = Address::generate(&env);
    env.ledger().set_timestamp(0);
    client.dao_grant(&dao, &creator, &token.address, &0, &1_000_000_000);
}

#[test]
#[should_panic(expected = "grant rate must be positive")]
fn test_dao_grant_zero_rate_panics() {
    let (env, client, token, dao, _admin) = setup();
    let creator = Address::generate(&env);
    env.ledger().set_timestamp(0);
    client.dao_grant(&dao, &creator, &token.address, &10_000, &0);
}

// ---------------------------------------------------------------------------
// creator_stats — grant registers as active fan
// ---------------------------------------------------------------------------

#[test]
fn test_dao_grant_registers_creator_support() {
    let (env, client, token, dao, _admin) = setup();
    let creator = Address::generate(&env);

    env.ledger().set_timestamp(0);
    client.dao_grant(&dao, &creator, &token.address, &10_000, &1_000_000_000);

    let stats = client.creator_stats(&creator);
    assert_eq!(stats.active_fans, 1);
    assert_eq!(stats.lifetime_fans, 1);
}
