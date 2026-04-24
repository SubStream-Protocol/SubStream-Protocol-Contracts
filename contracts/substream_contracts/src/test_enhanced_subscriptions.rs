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

fn setup_merchant_registry<'a>(
    env: &Env,
    client: &SubStreamContractClient<'a>,
    admin: &Address,
    merchant: &Address,
    token: &token::Client<'a>,
) {
    client.initialize(admin);
    let kyc_hash = vec![env; 32u8];
    let issuer = Address::from_string(&soroban_sdk::String::from_str(env, crate::SEP12_KYC_ISSUER));
    client.register_merchant_with_kyc(merchant, &kyc_hash, &issuer);
    client.set_accepted_token(merchant, &token.address());
}

fn approve_for_contract<'a>(
    env: &Env,
    token_admin: &token::StellarAssetClient<'a>,
    subscriber: &Address,
    contract_id: &Address,
    amount: i128,
) {
    let exp = env.ledger().sequence() + 1_000_000u32;
    token_admin.approve(subscriber, contract_id, &amount, &exp);
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

    setup_merchant_registry(&env, &client, &admin, &merchant, &token);

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
    client.initialize_subscription(&subscriber, &merchant, &1, &token.address(), &None);

    // Set up allowance
    token.approve(&subscriber, &contract_id, &1000, &1_000_000u32);

    env.ledger().set_timestamp(1000 + MONTH);

    client.execute_subscription_pull(&merchant, &subscriber, &0i128, &None);

    let events = env.events().all();
    assert_eq!(events.len(), 2);
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

    setup_merchant_registry(&env, &client, &admin, &merchant, &token);

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
    client.initialize_subscription(&subscriber, &merchant, &1, &token.address(), &None);

    env.ledger().set_timestamp(1000 + DAY);
    client.execute_subscription_pull(&merchant, &subscriber, &0i128, &None);
}

#[test]
fn test_execute_subscription_pull_insufficient_allowance() {
    let env = Env::default();
    env.mock_all_auths();

    let merchant = Address::generate(&env);
    let subscriber = Address::generate(&env);
    let admin = Address::generate(&env);

    let token = create_token_contract(&env, &admin);
    let contract_id = env.register(SubStreamContract, ());
    let client = SubStreamContractClient::new(&env, &contract_id);

    setup_merchant_registry(&env, &client, &admin, &merchant, &token);

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
    client.initialize_subscription(&subscriber, &merchant, &1, &token.address(), &None);

    env.ledger().set_timestamp(1000 + MONTH);
    client.execute_subscription_pull(&merchant, &subscriber, &0i128, &None);

    assert_eq!(
        client.get_subscription_status(&subscriber, &merchant),
        SubscriptionStatus::PastDue
    );
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

    setup_merchant_registry(&env, &client, &admin, &merchant, &token);

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

    env.ledger().set_timestamp(1000);
    client.initialize_subscription(&subscriber, &merchant, &1, &token.address(), &None);

    assert_eq!(
        client.get_subscription_status(&subscriber, &merchant),
        SubscriptionStatus::Trial
    );

    // Set up allowance for after trial
    token.approve(&subscriber, &contract_id, &1000, &1_000_000u32);

    env.ledger().set_timestamp(1000 + 7 * DAY + 1);

    client.execute_subscription_pull(&merchant, &subscriber, &0i128, &None);

    assert_eq!(
        client.get_subscription_status(&subscriber, &merchant),
        SubscriptionStatus::Active
    );

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

    setup_merchant_registry(&env, &client, &admin, &merchant, &token);

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

    env.ledger().set_timestamp(1000);
    client.initialize_subscription(&subscriber, &merchant, &1, &token.address(), &None);

    client.cancel(&subscriber, &merchant);

    client.initialize_subscription(&subscriber, &merchant, &1, &token.address(), &None);
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

    setup_merchant_registry(&env, &client, &admin, &merchant, &token);

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
    client.initialize_subscription(&subscriber, &merchant, &1, &token.address(), &None);

    env.ledger().set_timestamp(1000 + MONTH);

    client.execute_subscription_pull(&merchant, &subscriber, &0i128, &None);

    assert_eq!(
        client.get_subscription_status(&subscriber, &merchant),
        SubscriptionStatus::PastDue
    );

    let events = env.events().all();
    assert!(events.iter().any(|e| {
        matches!(e.topic_0, Some(topic) if topic == Symbol::from_str(&env, "PaymentFailedGracePeriodStarted"))
    }));

    // Now provide allowance and recover
    token.approve(&subscriber, &contract_id, &1000, &1_000_000u32);
    
    // Should succeed within grace period
    client.execute_subscription_pull(&merchant, &subscriber);

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

    setup_merchant_registry(&env, &client, &admin, &merchant, &token);

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
    client.initialize_subscription(&subscriber, &merchant, &1, &token.address(), &None);

    env.ledger().set_timestamp(1000 + MONTH);
    client.execute_subscription_pull(&merchant, &subscriber, &0i128, &None);

    env.ledger().set_timestamp(1000 + MONTH + 7 * DAY + 1);

    client.execute_subscription_pull(&merchant, &subscriber, &0i128, &None);
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

    setup_merchant_registry(&env, &client, &admin, &merchant, &token);

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

    env.ledger().set_timestamp(1000);
    client.initialize_subscription(&subscriber, &merchant, &1, &token.address(), &None);

    env.ledger().set_timestamp(1000 + MONTH / 2);

    client.upgrade_subscription_tier(&subscriber, &merchant, &2);

    let events = env.events().all();
    assert!(events.iter().any(|e| {
        matches!(e.topic_0, Some(topic) if topic == Symbol::from_str(&env, "SubscriptionUpgraded"))
    }));

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

    setup_merchant_registry(&env, &client, &admin, &merchant, &token);

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

    env.ledger().set_timestamp(1000);
    client.initialize_subscription(&subscriber, &merchant, &2, &token.address(), &None);

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

    setup_merchant_registry(&env, &client, &admin, &merchant, &token);

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

    env.ledger().set_timestamp(1000);
    client.initialize_subscription(&subscriber, &merchant, &1, &token.address(), &None);

    assert_eq!(
        client.get_subscription_status(&subscriber, &merchant),
        SubscriptionStatus::Trial
    );

    // Set up allowance for post-trial billing
    token.approve(&subscriber, &contract_id, &1000, &1_000_000u32);

    env.ledger().set_timestamp(1000 + 7 * DAY + 1);
    client.upgrade_subscription_tier(&subscriber, &merchant, &2);

    client.execute_subscription_pull(&merchant, &subscriber, &0i128, &None);

    assert_eq!(
        client.get_subscription_status(&subscriber, &merchant),
        SubscriptionStatus::Active
    );

    for i in 1..=3 {
        env.ledger().set_timestamp(1000 + 7 * DAY + 1 + (i as u64 * MONTH));
        client.execute_subscription_pull(&merchant, &subscriber, &0i128, &None);

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

    setup_merchant_registry(&env, &client, &admin, &merchant, &token);

    let basic_plan = Plan {
        plan_id: 1,
        name: soroban_sdk::String::from_str(&env, "Basic Plan"),
        billing_amount: 1,
        billing_cycle: MONTH,
        has_trial: false,
        trial_duration: 0,
        is_active: true,
    };
    client.register_plan(&merchant, basic_plan);

    let premium_plan = Plan {
        plan_id: 2,
        name: soroban_sdk::String::from_str(&env, "Premium Plan"),
        billing_amount: 3,
        billing_cycle: MONTH,
        has_trial: false,
        trial_duration: 0,
        is_active: true,
    };
    client.register_plan(&merchant, premium_plan);

    env.ledger().set_timestamp(1000);
    client.initialize_subscription(&subscriber, &merchant, &1, &token.address(), &None);

    env.ledger().set_timestamp(1000 + MONTH - 1);
    client.upgrade_subscription_tier(&subscriber, &merchant, &2);

    assert_eq!(
        client.get_subscription_status(&subscriber, &merchant),
        SubscriptionStatus::Active
    );
}
