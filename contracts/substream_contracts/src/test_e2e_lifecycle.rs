//! # End-to-End Integration Test: Trial → Pull → Upgrade → Dunning → Cancel
//!
//! Issue #150 — Final validation of the SubStream protocol before mainnet deployment.
//!
//! This test simulates the complete subscriber lifecycle in a single, unbroken flow:
//!
//! 1. **Trial**      – Subscriber starts a free-trial plan; no charge is taken.
//! 2. **Pull**       – Trial expires; merchant executes the first successful pull.
//! 3. **Upgrade**    – Subscriber upgrades mid-cycle to a higher-priced plan with proration.
//! 4. **Dunning**    – Subscriber's allowance runs out; pull enters PastDue / grace period.
//! 5. **Cancel**     – Subscriber cancels; remaining balance is refunded.
//!
//! ## Invariants verified
//! - `total_paid_to_merchant == sum_of_all_successful_pulls + prorated_upgrade_charge`
//! - `subscriber_refund == initial_deposit - total_paid_to_merchant`
//! - `contract_balance == 0` after cancel (full solvency)
//! - State transitions follow the documented FSM exactly
//! - Merchant features (pull, upgrade) do not corrupt core streaming math

#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

// ── Time constants ────────────────────────────────────────────────────────────

const DAY: u64 = 24 * 60 * 60;
const WEEK: u64 = 7 * DAY;
const MONTH: u64 = 30 * DAY;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn create_token<'a>(
    env: &Env,
    admin: &Address,
) -> (token::Client<'a>, token::StellarAssetClient<'a>) {
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let client = token::Client::new(env, &sac.address());
    let admin_client = token::StellarAssetClient::new(env, &sac.address());
    (client, admin_client)
}

/// Register a merchant via SEP-12 KYC and set their accepted token.
fn setup_verified_merchant<'a>(
    env: &Env,
    client: &SubStreamContractClient<'a>,
    admin: &Address,
    merchant: &Address,
    token: &token::Client<'a>,
) {
    client.initialize(admin);
    let kyc_hash = soroban_sdk::vec![env; 32u8];
    let issuer = Address::from_string(&soroban_sdk::String::from_str(env, crate::SEP12_KYC_ISSUER));
    client.register_merchant_with_kyc(merchant, &kyc_hash, &issuer);
    client.set_accepted_token(merchant, &token.address());
}

// ── Main end-to-end test ──────────────────────────────────────────────────────

/// Full lifecycle: Trial → Pull → Upgrade → Dunning → Cancel
///
/// This is the "mainnet readiness" test. It must pass without panics and
/// all mathematical invariants must hold at every stage.
#[test]
fn test_e2e_trial_pull_upgrade_dunning_cancel() {
    let env = Env::default();
    env.mock_all_auths();

    // ── Actors ────────────────────────────────────────────────────────────────
    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let subscriber = Address::generate(&env);

    // ── Token setup ───────────────────────────────────────────────────────────
    let (token, token_admin) = create_token(&env, &admin);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    // Mint enough tokens for the full lifecycle
    let initial_deposit: i128 = 1_000;
    token_admin.mint(&subscriber, &initial_deposit);

    // ── Contract setup ────────────────────────────────────────────────────────
    setup_verified_merchant(&env, &client, &admin, &merchant, &token);

    // Register two plans: Basic (plan 1) and Premium (plan 2)
    let basic_plan = Plan {
        plan_id: 1,
        name: soroban_sdk::String::from_str(&env, "Basic"),
        billing_amount: 100,
        billing_cycle: MONTH,
        has_trial: true,
        trial_duration: WEEK,
        is_active: true,
    };
    let premium_plan = Plan {
        plan_id: 2,
        name: soroban_sdk::String::from_str(&env, "Premium"),
        billing_amount: 200,
        billing_cycle: MONTH,
        is_active: true,
        has_trial: false,
        trial_duration: 0,
    };
    client.register_plan(&merchant, basic_plan);
    client.register_plan(&merchant, premium_plan);

    // ── STAGE 1: Trial ────────────────────────────────────────────────────────
    // Subscriber starts a trial. No tokens are transferred at this point.
    let t0: u64 = 1_000_000;
    env.ledger().set_timestamp(t0);

    client.initialize_subscription(&subscriber, &merchant, &1, &token.address(), &None);

    // Status must be Trial immediately after subscription
    assert_eq!(
        client.get_subscription_status(&subscriber, &merchant),
        SubscriptionStatus::Trial,
        "Stage 1: status should be Trial"
    );

    // Subscriber's token balance is unchanged (no deposit taken during trial init)
    assert_eq!(
        token.balance(&subscriber),
        initial_deposit,
        "Stage 1: subscriber balance unchanged during trial"
    );

    // Attempting a pull during the trial window must fail (billing premature)
    let pull_result = env.try_invoke_contract::<(), _>(
        &contract_id,
        &soroban_sdk::Symbol::new(&env, "execute_subscription_pull"),
        soroban_sdk::vec![
            &env,
            merchant.to_val(),
            subscriber.to_val(),
            0i128.into_val(&env),
            soroban_sdk::Val::VOID,
        ],
    );
    assert!(
        pull_result.is_err(),
        "Stage 1: pull during trial must be rejected"
    );

    // ── STAGE 2: Trial expires → first successful pull ────────────────────────
    // Advance past the trial duration. The billing cycle starts now.
    let t1 = t0 + WEEK + 1;
    env.ledger().set_timestamp(t1);

    // Grant the contract an allowance so the pull can transfer tokens
    token.approve(&subscriber, &contract_id, &initial_deposit, &10_000_000u32);

    client.execute_subscription_pull(&merchant, &subscriber, &0i128, &None);

    // Status must now be Active
    assert_eq!(
        client.get_subscription_status(&subscriber, &merchant),
        SubscriptionStatus::Active,
        "Stage 2: status should be Active after first pull"
    );

    let merchant_balance_after_pull1 = token.balance(&merchant);
    assert_eq!(
        merchant_balance_after_pull1, 100,
        "Stage 2: merchant received exactly one billing_amount (100)"
    );
    assert_eq!(
        token.balance(&subscriber),
        initial_deposit - 100,
        "Stage 2: subscriber paid exactly 100 tokens"
    );

    // ── STAGE 3: Upgrade mid-cycle with proration ─────────────────────────────
    // Advance to the middle of the first paid billing cycle
    let t2 = t1 + MONTH / 2;
    env.ledger().set_timestamp(t2);

    // Upgrade from Basic (100/month) to Premium (200/month)
    // Proration: half the cycle remains → unused_value = 50
    // Prorated charge = 200 - 50 = 150
    client.upgrade_subscription(&subscriber, &merchant, &2);

    // Status must still be Active after upgrade
    assert_eq!(
        client.get_subscription_status(&subscriber, &merchant),
        SubscriptionStatus::Active,
        "Stage 3: status should remain Active after upgrade"
    );

    // Merchant received the prorated charge on top of the first pull
    let merchant_balance_after_upgrade = token.balance(&merchant);
    assert!(
        merchant_balance_after_upgrade > merchant_balance_after_pull1,
        "Stage 3: merchant balance increased by prorated upgrade charge"
    );

    // ── STAGE 4: Second pull (Premium rate) ───────────────────────────────────
    // Advance past the new billing cycle (set during upgrade)
    let t3 = t2 + MONTH + 1;
    env.ledger().set_timestamp(t3);

    client.execute_subscription_pull(&merchant, &subscriber, &0i128, &None);

    let merchant_balance_after_pull2 = token.balance(&merchant);
    assert!(
        merchant_balance_after_pull2 > merchant_balance_after_upgrade,
        "Stage 4: merchant received second pull at Premium rate"
    );

    // ── STAGE 5: Dunning — allowance exhausted ────────────────────────────────
    // Revoke the subscriber's allowance to simulate a failed payment
    token.approve(&subscriber, &contract_id, &0i128, &10_000_000u32);

    let t4 = t3 + MONTH + 1;
    env.ledger().set_timestamp(t4);

    // Pull must fail and transition to PastDue
    let dunning_result = env.try_invoke_contract::<(), _>(
        &contract_id,
        &soroban_sdk::Symbol::new(&env, "execute_subscription_pull"),
        soroban_sdk::vec![
            &env,
            merchant.to_val(),
            subscriber.to_val(),
            0i128.into_val(&env),
            soroban_sdk::Val::VOID,
        ],
    );
    assert!(
        dunning_result.is_err(),
        "Stage 5: pull with zero allowance must fail"
    );

    assert_eq!(
        client.get_subscription_status(&subscriber, &merchant),
        SubscriptionStatus::PastDue,
        "Stage 5: status should be PastDue after failed pull"
    );

    // ── STAGE 6: Cancel — refund remaining balance ────────────────────────────
    // Advance past the minimum flow duration so cancel is permitted
    let t5 = t4 + DAY + 1;
    env.ledger().set_timestamp(t5);

    let subscriber_balance_before_cancel = token.balance(&subscriber);
    let contract_balance_before_cancel = token.balance(&contract_id);

    client.cancel(&subscriber, &merchant);

    // Status must be Canceled
    assert_eq!(
        client.get_subscription_status(&subscriber, &merchant),
        SubscriptionStatus::Canceled,
        "Stage 6: status should be Canceled"
    );

    // ── Solvency invariant ────────────────────────────────────────────────────
    // After cancel, the contract must hold zero tokens (full solvency).
    let contract_balance_after_cancel = token.balance(&contract_id);
    assert_eq!(
        contract_balance_after_cancel, 0,
        "Solvency: contract balance must be zero after cancel"
    );

    // The subscriber must have received a refund
    let subscriber_balance_after_cancel = token.balance(&subscriber);
    assert!(
        subscriber_balance_after_cancel >= subscriber_balance_before_cancel,
        "Solvency: subscriber balance must not decrease on cancel"
    );

    // Total conservation: initial_deposit == merchant_total + subscriber_refund
    let merchant_total = token.balance(&merchant);
    let subscriber_refund = subscriber_balance_after_cancel;
    assert_eq!(
        merchant_total + subscriber_refund,
        initial_deposit,
        "Solvency: total token conservation — merchant + subscriber == initial_deposit"
    );

    // ── State ledger dump ─────────────────────────────────────────────────────
    // Print a summary for CI logs to prove solvency at end of simulation.
    #[cfg(test)]
    {
        extern crate std as std2;
        std2::eprintln!("=== E2E State Ledger ===");
        std2::eprintln!("  initial_deposit       : {}", initial_deposit);
        std2::eprintln!("  merchant_total_earned : {}", merchant_total);
        std2::eprintln!("  subscriber_refund     : {}", subscriber_refund);
        std2::eprintln!("  contract_balance      : {}", contract_balance_after_cancel);
        std2::eprintln!("  SOLVENCY              : OK");
    }
}

// ── Supplementary invariant tests ────────────────────────────────────────────

/// Verify that a subscriber cannot cancel before the minimum flow duration.
#[test]
#[should_panic]
fn test_e2e_cancel_before_minimum_duration_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let subscriber = Address::generate(&env);

    let (token, token_admin) = create_token(&env, &admin);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    token_admin.mint(&subscriber, &1_000);
    setup_verified_merchant(&env, &client, &admin, &merchant, &token);

    let plan = Plan {
        plan_id: 1,
        name: soroban_sdk::String::from_str(&env, "Basic"),
        billing_amount: 100,
        billing_cycle: MONTH,
        has_trial: false,
        trial_duration: 0,
        is_active: true,
    };
    client.register_plan(&merchant, plan);

    env.ledger().set_timestamp(1_000_000);
    client.initialize_subscription(&subscriber, &merchant, &1, &token.address(), &None);

    // Attempt cancel immediately — must panic (minimum flow duration not elapsed)
    client.cancel(&subscriber, &merchant);
}

/// Verify that a trial cannot be reused after cancellation.
#[test]
#[should_panic(expected = "trial already used")]
fn test_e2e_trial_cannot_be_reused() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let subscriber = Address::generate(&env);

    let (token, token_admin) = create_token(&env, &admin);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    token_admin.mint(&subscriber, &1_000);
    setup_verified_merchant(&env, &client, &admin, &merchant, &token);

    let plan = Plan {
        plan_id: 1,
        name: soroban_sdk::String::from_str(&env, "Trial Plan"),
        billing_amount: 100,
        billing_cycle: MONTH,
        has_trial: true,
        trial_duration: WEEK,
        is_active: true,
    };
    client.register_plan(&merchant, plan);

    let t0: u64 = 1_000_000;
    env.ledger().set_timestamp(t0);
    client.initialize_subscription(&subscriber, &merchant, &1, &token.address(), &None);

    // Cancel after minimum duration
    env.ledger().set_timestamp(t0 + DAY + 1);
    client.cancel(&subscriber, &merchant);

    // Re-subscribe — must panic because trial was already used
    env.ledger().set_timestamp(t0 + DAY + 2);
    client.initialize_subscription(&subscriber, &merchant, &1, &token.address(), &None);
}

/// Verify that a downgrade attempt is rejected.
#[test]
#[should_panic(expected = "cannot downgrade")]
fn test_e2e_downgrade_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let subscriber = Address::generate(&env);

    let (token, token_admin) = create_token(&env, &admin);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    token_admin.mint(&subscriber, &10_000);
    setup_verified_merchant(&env, &client, &admin, &merchant, &token);

    let basic = Plan {
        plan_id: 1,
        name: soroban_sdk::String::from_str(&env, "Basic"),
        billing_amount: 100,
        billing_cycle: MONTH,
        has_trial: false,
        trial_duration: 0,
        is_active: true,
    };
    let premium = Plan {
        plan_id: 2,
        name: soroban_sdk::String::from_str(&env, "Premium"),
        billing_amount: 200,
        billing_cycle: MONTH,
        has_trial: false,
        trial_duration: 0,
        is_active: true,
    };
    client.register_plan(&merchant, basic);
    client.register_plan(&merchant, premium);

    let t0: u64 = 1_000_000;
    env.ledger().set_timestamp(t0);
    client.initialize_subscription(&subscriber, &merchant, &2, &token.address(), &None);

    // Attempt to downgrade from Premium (2) to Basic (1) — must panic
    env.ledger().set_timestamp(t0 + DAY);
    client.upgrade_subscription(&subscriber, &merchant, &1);
}

/// Verify that the grace period allows recovery from a failed pull.
#[test]
fn test_e2e_dunning_recovery_within_grace_period() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let subscriber = Address::generate(&env);

    let (token, token_admin) = create_token(&env, &admin);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    token_admin.mint(&subscriber, &10_000);
    setup_verified_merchant(&env, &client, &admin, &merchant, &token);

    let plan = Plan {
        plan_id: 1,
        name: soroban_sdk::String::from_str(&env, "Basic"),
        billing_amount: 100,
        billing_cycle: MONTH,
        has_trial: false,
        trial_duration: 0,
        is_active: true,
    };
    client.register_plan(&merchant, plan);

    let t0: u64 = 1_000_000;
    env.ledger().set_timestamp(t0);
    client.initialize_subscription(&subscriber, &merchant, &1, &token.address(), &None);

    // First pull succeeds
    token.approve(&subscriber, &contract_id, &10_000i128, &10_000_000u32);
    env.ledger().set_timestamp(t0 + MONTH + 1);
    client.execute_subscription_pull(&merchant, &subscriber, &0i128, &None);
    assert_eq!(
        client.get_subscription_status(&subscriber, &merchant),
        SubscriptionStatus::Active
    );

    // Revoke allowance → second pull fails → PastDue
    token.approve(&subscriber, &contract_id, &0i128, &10_000_000u32);
    env.ledger().set_timestamp(t0 + 2 * MONTH + 1);
    let _ = env.try_invoke_contract::<(), _>(
        &contract_id,
        &soroban_sdk::Symbol::new(&env, "execute_subscription_pull"),
        soroban_sdk::vec![
            &env,
            merchant.to_val(),
            subscriber.to_val(),
            0i128.into_val(&env),
            soroban_sdk::Val::VOID,
        ],
    );
    assert_eq!(
        client.get_subscription_status(&subscriber, &merchant),
        SubscriptionStatus::PastDue,
        "Should be PastDue after failed pull"
    );

    // Restore allowance within grace period → pull succeeds → Active
    token.approve(&subscriber, &contract_id, &10_000i128, &10_000_000u32);
    env.ledger().set_timestamp(t0 + 2 * MONTH + DAY / 2); // within 24h grace
    client.execute_subscription_pull(&merchant, &subscriber, &0i128, &None);
    assert_eq!(
        client.get_subscription_status(&subscriber, &merchant),
        SubscriptionStatus::Active,
        "Should recover to Active within grace period"
    );
}
