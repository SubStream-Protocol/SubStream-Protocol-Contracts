#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    testutils::Events as _,
    token, vec, Address, Env, Symbol,
};

const DAY: u64 = 24 * 60 * 60;
const WEEK: u64 = 7 * DAY;
const MONTH: u64 = 30 * DAY;

fn create_token_contract<'a>(env: &Env, admin: &Address) -> token::Client<'a> {
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    token::Client::new(env, &sac.address())
}

// ---------------------------------------------------------------------------
// #101: Automated Pull-Payment Execution Module Tests
// ---------------------------------------------------------------------------

#[test]
fn test_execute_subscription_pull_success() {
    let env = Env::default();
    env.mock_all_auths();

    let merchant = Address::generate(&env);
    let subscriber = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address());
    token_admin.mint(&subscriber, &1000000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    // Register a plan
    let plan = Plan {
        plan_id: 1,
        name: soroban_sdk::String::from_str(&env, "Basic Plan"),
        billing_amount: 100,
        billing_cycle: MONTH,
        has_trial: false,
        trial_duration: 0,
        is_active: true,
    };
    client.register_plan(&merchant, plan);

    // Initialize subscription
    env.ledger().set_timestamp(1000);
    client.initialize_subscription(&subscriber, &merchant, &1, &token.address());

    // Set up allowance
    token_client.approve(&subscriber, &contract_id, &1000);

    // Jump to billing date
    env.ledger().set_timestamp(1000 + MONTH);

    // Execute pull payment
    client.execute_subscription_pull(&merchant, &subscriber);

    // Verify events
    let events = env.events().all();
    assert_eq!(events.len(), 2); // Subscribed + SubscriptionBilled
}

#[test]
#[should_panic(expected = "billing premature")]
fn test_execute_subscription_pull_premature() {
    let env = Env::default();
    env.mock_all_auths();

    let merchant = Address::generate(&env);
    let subscriber = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    let plan = Plan {
        plan_id: 1,
        name: soroban_sdk::String::from_str(&env, "Basic Plan"),
        billing_amount: 100,
        billing_cycle: MONTH,
        has_trial: false,
        trial_duration: 0,
        is_active: true,
    };
    client.register_plan(&merchant, plan);

    env.ledger().set_timestamp(1000);
    client.initialize_subscription(&subscriber, &merchant, &1, &token.address());

    // Try to pull before billing date
    env.ledger().set_timestamp(1000 + DAY);
    client.execute_subscription_pull(&merchant, &subscriber);
}

#[test]
#[should_panic(expected = "insufficient allowance")]
fn test_execute_subscription_pull_insufficient_allowance() {
    let env = Env::default();
    env.mock_all_auths();

    let merchant = Address::generate(&env);
    let subscriber = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    let plan = Plan {
        plan_id: 1,
        name: soroban_sdk::String::from_str(&env, "Basic Plan"),
        billing_amount: 100,
        billing_cycle: MONTH,
        has_trial: false,
        trial_duration: 0,
        is_active: true,
    };
    client.register_plan(&merchant, plan);

    env.ledger().set_timestamp(1000);
    client.initialize_subscription(&subscriber, &merchant, &1, &token.address());

    // Don't set up allowance
    env.ledger().set_timestamp(1000 + MONTH);
    client.execute_subscription_pull(&merchant, &subscriber);
}

// ---------------------------------------------------------------------------
// #102: Enhanced Trial Period and Auto-Conversion Tests
// ---------------------------------------------------------------------------

#[test]
fn test_trial_period_auto_conversion() {
    let env = Env::default();
    env.mock_all_auths();

    let merchant = Address::generate(&env);
    let subscriber = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    // Register a trial plan
    let plan = Plan {
        plan_id: 1,
        name: soroban_sdk::String::from_str(&env, "Trial Plan"),
        billing_amount: 100,
        billing_cycle: MONTH,
        has_trial: true,
        trial_duration: 7 * DAY,
        is_active: true,
    };
    client.register_plan(&merchant, plan);

    // Initialize subscription with trial
    env.ledger().set_timestamp(1000);
    client.initialize_subscription(&subscriber, &merchant, &1, &token.address());

    // Verify trial status
    assert_eq!(
        client.get_subscription_status(&subscriber, &merchant),
        SubscriptionStatus::Trial
    );

    // Set up allowance for after trial
    token_client.approve(&subscriber, &contract_id, &1000);

    // Jump past trial period
    env.ledger().set_timestamp(1000 + 7 * DAY + 1);

    // Execute first pull payment (should convert from trial)
    client.execute_subscription_pull(&merchant, &subscriber);

    // Verify converted to active
    assert_eq!(
        client.get_subscription_status(&subscriber, &merchant),
        SubscriptionStatus::Active
    );

    // Verify trial events
    let events = env.events().all();
    assert!(events.iter().any(|e| {
        matches!(e.topic_0, Some(topic) if topic == Symbol::from_str(&env, "TrialStarted"))
    }));
}

#[test]
#[should_panic(expected = "trial already used")]
fn test_trial_abuse_prevention() {
    let env = Env::default();
    env.mock_all_auths();

    let merchant = Address::generate(&env);
    let subscriber = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    let plan = Plan {
        plan_id: 1,
        name: soroban_sdk::String::from_str(&env, "Trial Plan"),
        billing_amount: 100,
        billing_cycle: MONTH,
        has_trial: true,
        trial_duration: 7 * DAY,
        is_active: true,
    };
    client.register_plan(&merchant, plan);

    // First trial
    env.ledger().set_timestamp(1000);
    client.initialize_subscription(&subscriber, &merchant, &1, &token.address());

    // Cancel subscription
    client.cancel(&subscriber, &merchant);

    // Try to start another trial with same merchant
    client.initialize_subscription(&subscriber, &merchant, &1, &token.address());
}

// ---------------------------------------------------------------------------
// #103: Grace Period and Dunning Process Tests
// ---------------------------------------------------------------------------

#[test]
fn test_grace_period_entry_and_recovery() {
    let env = Env::default();
    env.mock_all_auths();

    let merchant = Address::generate(&env);
    let subscriber = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    let plan = Plan {
        plan_id: 1,
        name: soroban_sdk::String::from_str(&env, "Basic Plan"),
        billing_amount: 100,
        billing_cycle: MONTH,
        has_trial: false,
        trial_duration: 0,
        is_active: true,
    };
    client.register_plan(&merchant, plan);

    env.ledger().set_timestamp(1000);
    client.initialize_subscription(&subscriber, &merchant, &1, &token.address());

    // Jump to billing date but don't provide allowance
    env.ledger().set_timestamp(1000 + MONTH);

    // Try to pull - should fail and enter grace period
    let result = std::panic::catch_unwind(|| {
        client.execute_subscription_pull(&merchant, &subscriber);
    });
    assert!(result.is_err());

    // Verify in grace period
    assert_eq!(
        client.get_subscription_status(&subscriber, &merchant),
        SubscriptionStatus::PastDue
    );

    // Verify grace period started event
    let events = env.events().all();
    assert!(events.iter().any(|e| {
        matches!(e.topic_0, Some(topic) if topic == Symbol::from_str(&env, "PaymentFailedGracePeriodStarted"))
    }));

    // Now provide allowance and recover
    token_client.approve(&subscriber, &contract_id, &1000);
    
    // Should succeed within grace period
    client.execute_subscription_pull(&merchant, &subscriber);

    // Verify back to active
    assert_eq!(
        client.get_subscription_status(&subscriber, &merchant),
        SubscriptionStatus::Active
    );
}

#[test]
#[should_panic(expected = "grace period expired")]
fn test_grace_period_expiration() {
    let env = Env::default();
    env.mock_all_auths();

    let merchant = Address::generate(&env);
    let subscriber = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    let plan = Plan {
        plan_id: 1,
        name: soroban_sdk::String::from_str(&env, "Basic Plan"),
        billing_amount: 100,
        billing_cycle: MONTH,
        has_trial: false,
        trial_duration: 0,
        is_active: true,
    };
    client.register_plan(&merchant, plan);

    env.ledger().set_timestamp(1000);
    client.initialize_subscription(&subscriber, &merchant, &1, &token.address());

    // Jump to billing date and fail payment
    env.ledger().set_timestamp(1000 + MONTH);
    let _ = std::panic::catch_unwind(|| {
        client.execute_subscription_pull(&merchant, &subscriber);
    });

    // Jump past grace period (7 days)
    env.ledger().set_timestamp(1000 + MONTH + 7 * DAY + 1);

    // Try to pull - should fail due to expired grace period
    client.execute_subscription_pull(&merchant, &subscriber);
}

// ---------------------------------------------------------------------------
// #104: Tiered Subscription Upgrades and Proration Tests
// ---------------------------------------------------------------------------

#[test]
fn test_subscription_upgrade_with_proration() {
    let env = Env::default();
    env.mock_all_auths();

    let merchant = Address::generate(&env);
    let subscriber = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address());
    token_admin.mint(&subscriber, &1000000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    // Register basic plan
    let basic_plan = Plan {
        plan_id: 1,
        name: soroban_sdk::String::from_str(&env, "Basic Plan"),
        billing_amount: 100,
        billing_cycle: MONTH,
        has_trial: false,
        trial_duration: 0,
        is_active: true,
    };
    client.register_plan(&merchant, basic_plan);

    // Register pro plan
    let pro_plan = Plan {
        plan_id: 2,
        name: soroban_sdk::String::from_str(&env, "Pro Plan"),
        billing_amount: 200,
        billing_cycle: MONTH,
        has_trial: false,
        trial_duration: 0,
        is_active: true,
    };
    client.register_plan(&merchant, pro_plan);

    // Start with basic plan
    env.ledger().set_timestamp(1000);
    client.initialize_subscription(&subscriber, &merchant, &1, &token.address());

    // Jump halfway through billing cycle
    env.ledger().set_timestamp(1000 + MONTH / 2);

    // Upgrade to pro plan
    client.upgrade_subscription_tier(&subscriber, &merchant, &2);

    // Verify upgrade event
    let events = env.events().all();
    assert!(events.iter().any(|e| {
        matches!(e.topic_0, Some(topic) if topic == Symbol::from_str(&env, "SubscriptionUpgraded"))
    }));

    // Verify new billing amount
    let billing_key = DataKey::BillingCycle(subscriber.clone(), merchant.clone());
    let billing_info: BillingCycleInfo = env.storage().persistent().get(&billing_key).unwrap();
    assert_eq!(billing_info.billing_amount, 200);
}

#[test]
#[should_panic(expected = "cannot downgrade")]
fn test_prevent_downgrade() {
    let env = Env::default();
    env.mock_all_auths();

    let merchant = Address::generate(&env);
    let subscriber = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    // Register pro plan
    let pro_plan = Plan {
        plan_id: 2,
        name: soroban_sdk::String::from_str(&env, "Pro Plan"),
        billing_amount: 200,
        billing_cycle: MONTH,
        has_trial: false,
        trial_duration: 0,
        is_active: true,
    };
    client.register_plan(&merchant, pro_plan);

    // Register basic plan
    let basic_plan = Plan {
        plan_id: 1,
        name: soroban_sdk::String::from_str(&env, "Basic Plan"),
        billing_amount: 100,
        billing_cycle: MONTH,
        has_trial: false,
        trial_duration: 0,
        is_active: true,
    };
    client.register_plan(&merchant, basic_plan);

    // Start with pro plan
    env.ledger().set_timestamp(1000);
    client.initialize_subscription(&subscriber, &merchant, &2, &token.address());

    // Try to downgrade to basic plan
    client.upgrade_subscription_tier(&subscriber, &merchant, &1);
}

// ---------------------------------------------------------------------------
// Integration Tests
// ---------------------------------------------------------------------------

#[test]
fn test_full_subscription_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let merchant = Address::generate(&env);
    let subscriber = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address());
    token_admin.mint(&subscriber, &1000000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    // Register trial and paid plans
    let trial_plan = Plan {
        plan_id: 1,
        name: soroban_sdk::String::from_str(&env, "Trial Plan"),
        billing_amount: 100,
        billing_cycle: MONTH,
        has_trial: true,
        trial_duration: 7 * DAY,
        is_active: true,
    };
    client.register_plan(&merchant, trial_plan);

    let pro_plan = Plan {
        plan_id: 2,
        name: soroban_sdk::String::from_str(&env, "Pro Plan"),
        billing_amount: 200,
        billing_cycle: MONTH,
        has_trial: false,
        trial_duration: 0,
        is_active: true,
    };
    client.register_plan(&merchant, pro_plan);

    // Start trial
    env.ledger().set_timestamp(1000);
    client.initialize_subscription(&subscriber, &merchant, &1, &token.address());

    // Verify trial status
    assert_eq!(
        client.get_subscription_status(&subscriber, &merchant),
        SubscriptionStatus::Trial
    );

    // Set up allowance for post-trial billing
    token_client.approve(&subscriber, &contract_id, &1000);

    // Jump past trial and upgrade to pro
    env.ledger().set_timestamp(1000 + 7 * DAY + 1);
    client.upgrade_subscription_tier(&subscriber, &merchant, &2);

    // Execute first billing
    client.execute_subscription_pull(&merchant, &subscriber);

    // Verify active status
    assert_eq!(
        client.get_subscription_status(&subscriber, &merchant),
        SubscriptionStatus::Active
    );

    // Continue with normal billing cycles
    for i in 1..=3 {
        env.ledger().set_timestamp(1000 + 7 * DAY + 1 + (i as u64 * MONTH));
        client.execute_subscription_pull(&merchant, &subscriber);
        
        assert_eq!(
            client.get_subscription_status(&subscriber, &merchant),
            SubscriptionStatus::Active
        );
    }
}

#[test]
fn test_proration_math_edge_cases() {
    let env = Env::default();
    env.mock_all_auths();

    let merchant = Address::generate(&env);
    let subscriber = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let token_admin = token::StellarAssetClient::new(&env, &token.address());
    token_admin.mint(&subscriber, &1000000);

    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    // Test with very small amounts to check rounding
    let basic_plan = Plan {
        plan_id: 1,
        name: soroban_sdk::String::from_str(&env, "Basic Plan"),
        billing_amount: 1, // 1 token
        billing_cycle: MONTH,
        has_trial: false,
        trial_duration: 0,
        is_active: true,
    };
    client.register_plan(&merchant, basic_plan);

    let premium_plan = Plan {
        plan_id: 2,
        name: soroban_sdk::String::from_str(&env, "Premium Plan"),
        billing_amount: 3, // 3 tokens
        billing_cycle: MONTH,
        has_trial: false,
        trial_duration: 0,
        is_active: true,
    };
    client.register_plan(&merchant, premium_plan);

    // Start with basic plan
    env.ledger().set_timestamp(1000);
    client.initialize_subscription(&subscriber, &merchant, &1, &token.address());

    // Upgrade very late in cycle (should charge full difference)
    env.ledger().set_timestamp(1000 + MONTH - 1);
    client.upgrade_subscription_tier(&subscriber, &merchant, &2);

    // Verify upgrade completed successfully
    assert_eq!(
        client.get_subscription_status(&subscriber, &merchant),
        SubscriptionStatus::Active
    );
}
