/// Issue #124: Native DAO Treasury Token Buyback Hook
/// Tests covering:
/// Acceptance 1: Protocol revenue automatically accrues value to governance.
/// Acceptance 2: Cross-contract DEX interactions secured against front-running.
/// Acceptance 3: Relayers incentivised via hardcoded gas bounty.
#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

fn setup() -> (
    Env,
    SubStreamContractClient<'static>,
    token::Client<'static>,
    token::StellarAssetClient<'static>,
    Address, // admin
    Address, // dao_treasury
    Address, // dex_router (stub address)
    Address, // governance_token (stub address)
) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let dao_treasury = Address::generate(&env);
    let dex_router = Address::generate(&env);

    // Payment token (e.g. USDC)
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let payment_token = token::Client::new(&env, &sac.address());
    let payment_token_admin = token::StellarAssetClient::new(&env, &sac.address());

    // Governance token address (stub — in tests we only check events/balances)
    let governance_sac = env.register_stellar_asset_contract_v2(admin.clone());
    let governance_token_addr = governance_sac.address();

    // Fund DAO treasury
    payment_token_admin.mint(&dao_treasury, &1_000_000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    let client: SubStreamContractClient<'static> = unsafe { core::mem::transmute(client) };
    let payment_token: token::Client<'static> = unsafe { core::mem::transmute(payment_token) };
    let payment_token_admin: token::StellarAssetClient<'static> =
        unsafe { core::mem::transmute(payment_token_admin) };

    (
        env,
        client,
        payment_token,
        payment_token_admin,
        admin,
        dao_treasury,
        dex_router,
        governance_token_addr,
    )
}

// ---------------------------------------------------------------------------
// configure_buyback
// ---------------------------------------------------------------------------

#[test]
fn test_configure_buyback_happy_path() {
    let (env, client, payment_token, _pt_admin, admin, dao_treasury, dex_router, gov_token) =
        setup();

    client.configure_buyback(
        &admin,
        &dao_treasury,
        &dex_router,
        &gov_token,
        &payment_token.address,
        &100_000, // trigger_threshold
        &500,     // relayer_bounty
        &50,      // max_slippage_bps (0.5%)
    );

    let config = client.get_buyback_config();
    assert_eq!(config.dao_treasury, dao_treasury);
    assert_eq!(config.dex_router, dex_router);
    assert_eq!(config.governance_token, gov_token);
    assert_eq!(config.payment_token, payment_token.address);
    assert_eq!(config.trigger_threshold, 100_000);
    assert_eq!(config.relayer_bounty, 500);
    assert_eq!(config.max_slippage_bps, 50);
    assert!(config.enabled);
}

#[test]
#[should_panic(expected = "admin only")]
fn test_configure_buyback_non_admin_panics() {
    let (env, client, payment_token, _pt_admin, _admin, dao_treasury, dex_router, gov_token) =
        setup();
    let faker = Address::generate(&env);
    client.configure_buyback(
        &faker,
        &dao_treasury,
        &dex_router,
        &gov_token,
        &payment_token.address,
        &100_000,
        &500,
        &50,
    );
}

#[test]
#[should_panic(expected = "trigger threshold must be positive")]
fn test_configure_buyback_zero_threshold_panics() {
    let (env, client, payment_token, _pt_admin, admin, dao_treasury, dex_router, gov_token) =
        setup();
    let _ = env;
    client.configure_buyback(
        &admin,
        &dao_treasury,
        &dex_router,
        &gov_token,
        &payment_token.address,
        &0,
        &500,
        &50,
    );
}

// ---------------------------------------------------------------------------
// Acceptance 2: Front-run protection via nonce commitment
// ---------------------------------------------------------------------------

#[test]
fn test_commit_and_trigger_buyback() {
    let (env, client, payment_token, _pt_admin, admin, dao_treasury, dex_router, gov_token) =
        setup();

    client.configure_buyback(
        &admin,
        &dao_treasury,
        &dex_router,
        &gov_token,
        &payment_token.address,
        &100_000, // threshold
        &500,     // bounty
        &50,
    );

    let relayer = Address::generate(&env);
    let nonce: u64 = 42;

    // Step 1: relayer commits nonce
    client.commit_buyback_nonce(&relayer, &nonce);

    env.ledger().set_timestamp(1000);

    // Step 2: relayer triggers buyback
    // dao_treasury has 1_000_000 tokens, threshold = 100_000 → should fire
    client.trigger_buyback(&relayer, &nonce, &0);

    // Treasury should have decreased by swap_amount + bounty
    let remaining = payment_token.balance(&dao_treasury);
    // swap_amount = 1_000_000 - 500 = 999_500 transferred to dex_router
    // bounty = 500 transferred to relayer
    assert_eq!(remaining, 0);
    assert_eq!(payment_token.balance(&relayer), 500);
    assert_eq!(payment_token.balance(&dex_router), 999_500);
}

#[test]
#[should_panic(expected = "nonce not committed")]
fn test_trigger_without_commit_panics() {
    let (env, client, payment_token, _pt_admin, admin, dao_treasury, dex_router, gov_token) =
        setup();
    let _ = env;
    client.configure_buyback(
        &admin,
        &dao_treasury,
        &dex_router,
        &gov_token,
        &payment_token.address,
        &100_000,
        &500,
        &50,
    );
    let relayer = Address::generate(&env);
    client.trigger_buyback(&relayer, &99, &0); // nonce 99 never committed
}

#[test]
#[should_panic(expected = "nonce belongs to different relayer")]
fn test_trigger_with_wrong_relayer_panics() {
    let (env, client, payment_token, _pt_admin, admin, dao_treasury, dex_router, gov_token) =
        setup();
    client.configure_buyback(
        &admin,
        &dao_treasury,
        &dex_router,
        &gov_token,
        &payment_token.address,
        &100_000,
        &500,
        &50,
    );
    let relayer_a = Address::generate(&env);
    let relayer_b = Address::generate(&env);
    let nonce: u64 = 7;
    client.commit_buyback_nonce(&relayer_a, &nonce);
    // relayer_b tries to steal the nonce → must panic
    client.trigger_buyback(&relayer_b, &nonce, &0);
}

#[test]
#[should_panic(expected = "nonce already committed")]
fn test_duplicate_nonce_commitment_panics() {
    let (env, client, _pt, _pta, _admin, _dao, _dex, _gov) = setup();
    let _ = env;
    let relayer = Address::generate(&env);
    client.commit_buyback_nonce(&relayer, &1);
    client.commit_buyback_nonce(&relayer, &1); // duplicate
}

#[test]
#[should_panic(expected = "treasury balance below trigger threshold")]
fn test_trigger_below_threshold_panics() {
    let (env, client, payment_token, pt_admin, admin, dao_treasury, dex_router, gov_token) =
        setup();

    // Set threshold higher than available balance
    client.configure_buyback(
        &admin,
        &dao_treasury,
        &dex_router,
        &gov_token,
        &payment_token.address,
        &2_000_000, // higher than the 1_000_000 we minted
        &500,
        &50,
    );
    let _ = pt_admin;

    let relayer = Address::generate(&env);
    client.commit_buyback_nonce(&relayer, &5);
    client.trigger_buyback(&relayer, &5, &0);
}

// ---------------------------------------------------------------------------
// Acceptance 3: Relayer bounty is paid
// ---------------------------------------------------------------------------

#[test]
fn test_relayer_receives_bounty() {
    let (env, client, payment_token, _pt_admin, admin, dao_treasury, dex_router, gov_token) =
        setup();

    let bounty = 1_000i128;
    client.configure_buyback(
        &admin,
        &dao_treasury,
        &dex_router,
        &gov_token,
        &payment_token.address,
        &100_000,
        &bounty,
        &50,
    );

    let relayer = Address::generate(&env);
    client.commit_buyback_nonce(&relayer, &10);
    client.trigger_buyback(&relayer, &10, &0);

    assert_eq!(payment_token.balance(&relayer), bounty);
}
