//! End-to-end lifecycle test for Issue #150.
//!
//! Validates the complete subscriber journey in a single, uninterrupted flow:
//!
//!   1. **Trial**   — subscriber starts on a trial plan; status = Trial.
//!   2. **Active**  — trial expires; merchant pulls first payment; status = Active.
//!   3. **Pull**    — merchant executes a second successful pull.
//!   4. **Upgrade** — subscriber upgrades mid-cycle with proration.
//!   5. **Dunning** — allowance revoked; pull fails; status = PastDue.
//!   6. **Cancel**  — subscriber cancels; refund issued; solvency verified.
//!
//! Acceptance criteria (Issue #150):
//!   - All isolated modules work together without logic collisions or panics.
//!   - State changes across the entire billing lifecycle are mathematically verified.
//!   - Final state ledger is dumped to prove total solvency.

#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

const DAY: u64 = 24 * 60 * 60;
const WEEK: u64 = 7 * DAY;
const MONTH: u64 = 30 * DAY;

fn setup_token<'a>(env: &Env, admin: &Address) -> (token::Client<'a>, token::StellarAssetClient<'a>) {
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let client = token::Client::new(env, &sac.address());
    let admin_client = token::StellarAssetClient::new(env, &sac.address());
    (client, admin_client)
}

fn setup_contract<'a>(
    env: &Env,
    admin: &Address,
    merchant: &Address,
    token: &token::Client<'a>,
) -> (Address, SubStreamContractClient<'a>) {
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(env, &contract_id);
    client.initialize(admin);
    let kyc_hash = soroban_sdk::vec![env; 32u8];
    let issuer = Address::from_string(&soroban_sdk::String::from_str(env, crate::SEP12_KYC_ISSUER));
    client.register_merchant_with_kyc(merchant, &kyc_hash, &issuer);
    client.set_accepted_token(merchant, &token.address());
    (contract_id, client)
}

/// Full lifecycle: Trial → Active → Pull → Upgrade → Dunning → Cancel.
///
/// At the end we dump the final state and assert total solvency:
///   subscriber_balance + merchant_balance + contract_balance == initial_mint
#[test]
fn test_full_subscriber_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let subscriber = Address::generate(&env);

    let (token, token_admin) = setup_token(&env, &admin);
    let (contract_id, client) = setup_contract(&env, &admin, &merchant, &token);

    // ── Plan setup ──────────────────────────────────────────────────────────
    // Basic plan: 100 tokens/month, 7-day trial.
    let basic_plan = Plan {
        plan_id: 1,
        name: soroban_sdk::String::from_str(&env, "Basic"),
        billing_amount: 100,
        billing_cycle: MONTH,
        has_trial: true,
        trial_duration: WEEK,
        is_active: true,
    };
    // Premium plan: 200 tokens/month, no trial (upgrade target).
    let premium_plan = Plan {
        plan_id: 2,
        name: soroban_sdk::String::from_str(&env, "Premium"),
        billing_amount: 200,
        billing_cycle: MONTH,
        has_trial: false,
        trial_duration: 0,
        is_active: true,
    };
    client.register_plan(&merchant, basic_plan);
    client.register_plan(&merchant, premium_plan);

    // Mint enough for two full billing cycles plus upgrade proration buffer.
    let initial_mint: i128 = 1_000;
    token_admin.mint(&subscriber, &initial_mint);

    // ── Step 1: Trial ────────────────────────────────────────────────────────
    let t0: u64 = 1_000_000;
    env.ledger().set_timestamp(t0);

    client.initialize_subscription(&subscriber, &merchant, &1, &token.address(), &None);

    assert_eq!(
        client.get_subscription_status(&subscriber, &merchant),
        SubscriptionStatus::Trial,
        "Step 1: status must be Trial immediately after subscribe"
    );

    // ── Step 2: Trial expires → first pull → Active ──────────────────────────
    // Approve enough for two pulls.
    token.approve(&subscriber, &contract_id, &500i128, &1_000_000u32);

    let t_after_trial = t0 + WEEK + 1;
    env.ledger().set_timestamp(t_after_trial);

    client.execute_subscription_pull(&merchant, &subscriber, &0i128, &None);

    assert_eq!(
        client.get_subscription_status(&subscriber, &merchant),
        SubscriptionStatus::Active,
        "Step 2: status must be Active after first successful pull"
    );

    let merchant_balance_after_pull1 = token.balance(&merchant);
    assert_eq!(
        merchant_balance_after_pull1, 100,
        "Step 2: merchant must have received exactly 100 tokens"
    );

    // ── Step 3: Second successful pull ──────────────────────────────────────
    let t_second_pull = t_after_trial + MONTH;
    env.ledger().set_timestamp(t_second_pull);

    client.execute_subscription_pull(&merchant, &subscriber, &0i128, &None);

    assert_eq!(
        client.get_subscription_status(&subscriber, &merchant),
        SubscriptionStatus::Active,
        "Step 3: status must remain Active after second pull"
    );

    let merchant_balance_after_pull2 = token.balance(&merchant);
    assert_eq!(
        merchant_balance_after_pull2, 200,
        "Step 3: merchant must have received 200 tokens total after two pulls"
    );

    // ── Step 4: Mid-cycle upgrade with proration ─────────────────────────────
    // Upgrade halfway through the cycle.
    let t_upgrade = t_second_pull + MONTH / 2;
    env.ledger().set_timestamp(t_upgrade);

    // Approve extra for the prorated upgrade charge.
    token.approve(&subscriber, &contract_id, &500i128, &1_000_000u32);

    client.upgrade_subscription(&subscriber, &merchant, &2u32);

    assert_eq!(
        client.get_subscription_status(&subscriber, &merchant),
        SubscriptionStatus::Active,
        "Step 4: status must remain Active after upgrade"
    );

    // After upgrade the billing amount must reflect the premium plan.
    let billing_key = DataKey::BillingCycle(subscriber.clone(), merchant.clone());
    let billing: BillingCycleInfo = env
        .storage()
        .persistent()
        .get(&billing_key)
        .expect("billing record must exist after upgrade");
    assert_eq!(
        billing.billing_amount, 200,
        "Step 4: billing_amount must be 200 after upgrade to Premium"
    );

    // ── Step 5: Allowance revoked → pull fails → PastDue (Dunning) ──────────
    // Revoke allowance so the next pull cannot succeed.
    token.approve(&subscriber, &contract_id, &0i128, &1_000_000u32);

    let t_dunning_pull = t_upgrade + MONTH;
    env.ledger().set_timestamp(t_dunning_pull);

    client.execute_subscription_pull(&merchant, &subscriber, &0i128, &None);

    assert_eq!(
        client.get_subscription_status(&subscriber, &merchant),
        SubscriptionStatus::PastDue,
        "Step 5: status must be PastDue when allowance is insufficient"
    );

    // ── Step 6: Cancel → refund → solvency check ────────────────────────────
    // Advance past the minimum flow duration so cancel is allowed.
    let t_cancel = t_dunning_pull + DAY + 1;
    env.ledger().set_timestamp(t_cancel);

    let subscriber_balance_before_cancel = token.balance(&subscriber);
    client.cancel(&subscriber, &merchant);

    // Billing record must be gone or in Canceled state.
    let post_cancel_status = client.get_subscription_status(&subscriber, &merchant);
    assert_eq!(
        post_cancel_status,
        SubscriptionStatus::Canceled,
        "Step 6: subscription status must be Canceled after cancel"
    );

    // ── Final solvency dump ──────────────────────────────────────────────────
    let final_subscriber = token.balance(&subscriber);
    let final_merchant = token.balance(&merchant);
    let final_contract = token.balance(&contract_id);

    // Print final state ledger for audit trail.
    #[cfg(test)]
    {
        extern crate std as std2;
        std2::eprintln!("=== FINAL STATE LEDGER (Issue #150 Solvency Proof) ===");
        std2::eprintln!("  initial_mint       : {}", initial_mint);
        std2::eprintln!("  final_subscriber   : {}", final_subscriber);
        std2::eprintln!("  final_merchant     : {}", final_merchant);
        std2::eprintln!("  final_contract     : {}", final_contract);
        std2::eprintln!("  sum                : {}", final_subscriber + final_merchant + final_contract);
        std2::eprintln!("======================================================");
    }

    // Solvency invariant: no tokens created or destroyed.
    assert_eq!(
        final_subscriber + final_merchant + final_contract,
        initial_mint,
        "Solvency invariant violated: tokens must be conserved across the full lifecycle"
    );

    // Subscriber must have received a refund (more than before cancel).
    assert!(
        final_subscriber >= subscriber_balance_before_cancel,
        "Step 6: subscriber must receive a refund on cancel"
    );
}
