//! Pull-based billing, pending settlement, and DAO-juror dispute resolution.

use crate::{
    BillingCycleInfo, DataKey, DisputeRaised, DisputeRecord, DisputeResolved, JurorSignature,
    PendingMerchantPullInfo, Plan, Subscription, SubscriptionBilled, SubscriptionStatus, Subscribed,
    Tier, TokenClient, TrialConverted, TrialStarted, PaymentFailedGracePeriodStarted,
    FREE_TRIAL_DURATION, GRACE_PERIOD, MINIMUM_FLOW_DURATION,
};
use core::borrow::Borrow;
use soroban_sdk::{vec, Address, Bytes, BytesN, Env, Vec};

pub fn configure_dispute_jurors(env: &Env, admin: &Address, juror_pubkeys: Vec<BytesN<32>>) {
    admin.require_auth();
    let stored_admin: Address = env
        .storage()
        .persistent()
        .get(&DataKey::ContractAdmin)
        .expect("not initialized");
    if admin != &stored_admin {
        panic!("admin only");
    }
    env.storage()
        .persistent()
        .set(&DataKey::DisputeJurorKeys, &juror_pubkeys);
}

pub fn dispute_verdict_digest(env: &Env, dispute_id: u64, user_wins: bool) -> BytesN<32> {
    let mut buf = Bytes::new(env);
    for b in dispute_id.to_be_bytes() {
        buf.push_back(b);
    }
    buf.push_back(if user_wins { 1u8 } else { 0u8 });
    env.crypto().sha256(&buf)
}

fn next_dispute_id(env: &Env) -> u64 {
    let id: u64 = env
        .storage()
        .persistent()
        .get(&DataKey::NextDisputeId)
        .unwrap_or(1);
    env.storage()
        .persistent()
        .set(&DataKey::NextDisputeId, &(id.saturating_add(1)));
    id
}

fn juror_is_registered(env: &Env, pk: &BytesN<32>) -> bool {
    let jurors: Vec<BytesN<32>> = env
        .storage()
        .persistent()
        .get(&DataKey::DisputeJurorKeys)
        .unwrap_or_else(|| vec![env]);
    for j in jurors.iter() {
        if j == *pk {
            return true;
        }
    }
    false
}

fn verify_juror_threshold(
    env: &Env,
    dispute_id: u64,
    user_wins: bool,
    sigs: Vec<JurorSignature>,
) {
    let digest = dispute_verdict_digest(env, dispute_id, user_wins);
    let mut valid: u32 = 0;
    let mut seen = vec![env];
    for i in 0..sigs.len() {
        let entry = sigs.get(i).unwrap();
        if !juror_is_registered(env, &entry.pubkey) {
            panic!("unknown juror");
        }
        let mut dup = false;
        for k in 0..seen.len() {
            if seen.get(k).unwrap() == entry.pubkey {
                dup = true;
                break;
            }
        }
        if dup {
            panic!("duplicate juror signature");
        }
        seen.push_back(entry.pubkey.clone());
        env.crypto()
            .ed25519_verify(&entry.pubkey, digest.borrow(), &entry.sig);
        valid = valid.saturating_add(1);
    }
    if valid < crate::DAO_MULTISIG_THRESHOLD {
        panic!("insufficient juror signatures");
    }
}

pub fn maybe_release_expired_pending_pull(
    env: &Env,
    subscriber: &Address,
    merchant: &Address,
    contract: &Address,
) {
    let pending_key = DataKey::PendingMerchantPull(subscriber.clone(), merchant.clone());
    let Some(pending): Option<PendingMerchantPullInfo> =
        env.storage().persistent().get(&pending_key)
    else {
        return;
    };
    let now = env.ledger().timestamp();
    if now.saturating_sub(pending.pulled_at) < crate::DISPUTE_WINDOW_SEC {
        return;
    }
    if env
        .storage()
        .persistent()
        .has(&DataKey::ActiveDispute(subscriber.clone(), merchant.clone()))
    {
        return;
    }
    let token_client = TokenClient::new(env, &pending.token);
    if pending.amount > 0 {
        token_client.transfer(contract, merchant, &pending.amount);
    }
    env.storage().persistent().remove(&pending_key);
}

pub fn initialize_subscription(
    env: &Env,
    subscriber: Address,
    merchant: Address,
    plan_id: u32,
    token: Address,
) {
    subscriber.require_auth();
    let billing_key = DataKey::BillingCycle(subscriber.clone(), merchant.clone());
    if env.storage().persistent().has(&billing_key) {
        panic!("subscription exists");
    }

    let plan_registry_key = DataKey::PlanRegistry(merchant.clone());
    let plans: Vec<Plan> = env
        .storage()
        .persistent()
        .get(&plan_registry_key)
        .expect("no plans for merchant");

    let mut chosen: Option<Plan> = None;
    for p in plans.iter() {
        if p.plan_id == plan_id && p.is_active {
            chosen = Some(p);
            break;
        }
    }
    let plan = chosen.expect("plan not found");

    if plan.has_trial {
        let trial_key = DataKey::TrialUsed(subscriber.clone(), merchant.clone());
        if env.storage().persistent().has(&trial_key) {
            panic!("trial already used");
        }
        env.storage().persistent().set(&trial_key, &true);
    }

    let now = env.ledger().timestamp();
    let sub_start = now.saturating_sub(MINIMUM_FLOW_DURATION + 1);
    let status = if plan.has_trial {
        SubscriptionStatus::Trial
    } else {
        SubscriptionStatus::Active
    };
    let next_billing = if plan.has_trial {
        now.saturating_add(plan.trial_duration)
    } else {
        now.saturating_add(plan.billing_cycle)
    };

    let billing_info = BillingCycleInfo {
        next_billing_date: next_billing,
        dunning_start_timestamp: 0,
        status,
        billing_amount: plan.billing_amount,
        billing_cycle: plan.billing_cycle,
    };
    env.storage()
        .persistent()
        .set(&billing_key, &billing_info);

    let rate = plan.billing_amount / plan.billing_cycle as i128;
    let trial_duration = if plan.has_trial {
        plan.trial_duration
    } else {
        FREE_TRIAL_DURATION
    };

    let sub = Subscription {
        token: token.clone(),
        tier: Tier {
            rate_per_second: rate,
            trial_duration,
        },
        balance: 0,
        last_collected: sub_start,
        start_time: sub_start,
        streak_start_date: sub_start,
        last_funds_exhausted: 0,
        flags: 0,
        creators: vec![env, merchant.clone()],
        percentages: vec![env, 100u32],
        payer: subscriber.clone(),
        beneficiary: subscriber.clone(),
        accrued_remainder: 0,
    };
    let sub_key = crate::subscription_key(&subscriber, &merchant);
    crate::set_subscription(env, &sub_key, &sub);

    let mut total_flow: i128 = env
        .storage()
        .persistent()
        .get(&DataKey::CurrentFlowRate(merchant.clone()))
        .unwrap_or(0);
    total_flow = total_flow.saturating_add(rate);
    env
        .storage()
        .persistent()
        .set(&DataKey::CurrentFlowRate(merchant.clone()), &total_flow);

    crate::register_creator_support(env, &merchant, &subscriber);

    if plan.has_trial {
        TrialStarted {
            subscriber: subscriber.clone(),
            merchant: merchant.clone(),
            trial_duration: plan.trial_duration,
            started_at: now,
        }
        .publish(env);
    }

    Subscribed {
        subscriber: subscriber.clone(),
        creator: merchant.clone(),
        rate_per_second: rate,
    }
    .publish(env);
}

pub fn execute_subscription_pull(env: &Env, merchant: Address, subscriber: Address) {
    merchant.require_auth();
    let contract = env.current_contract_address();
    let billing_key = DataKey::BillingCycle(subscriber.clone(), merchant.clone());
    let mut billing: BillingCycleInfo = env
        .storage()
        .persistent()
        .get(&billing_key)
        .expect("no billing subscription");

    if billing.status == SubscriptionStatus::Disputed {
        panic!("subscription disputed");
    }
    if billing.status == SubscriptionStatus::Canceled {
        panic!("subscription canceled");
    }

    maybe_release_expired_pending_pull(env, &subscriber, &merchant, &contract);

    let now = env.ledger().timestamp();
    if now < billing.next_billing_date {
        panic!("billing premature");
    }

    let sub_key = crate::subscription_key(&subscriber, &merchant);
    let sub = crate::get_subscription(env, &sub_key);
    let token_client = TokenClient::new(env, &sub.token);
    let amount = billing.billing_amount;

    let allowance = token_client.allowance(&subscriber, &contract);
    if allowance < amount {
        if billing.status != SubscriptionStatus::PastDue {
            billing.status = SubscriptionStatus::PastDue;
            billing.dunning_start_timestamp = now;
            env.storage().persistent().set(&billing_key, &billing);
            PaymentFailedGracePeriodStarted {
                subscriber: subscriber.clone(),
                merchant: merchant.clone(),
                dunning_start_timestamp: now,
                grace_period_end: now.saturating_add(GRACE_PERIOD),
            }
            .publish(env);
        }
        panic!("insufficient allowance");
    }

    if billing.status == SubscriptionStatus::PastDue {
        if now > billing.dunning_start_timestamp.saturating_add(GRACE_PERIOD) {
            panic!("grace period expired");
        }
    }

    token_client.transfer_from(&contract, &subscriber, &contract, &amount);

    let pulled_at = now;
    let pending = PendingMerchantPullInfo {
        amount,
        token: sub.token.clone(),
        pulled_at,
    };
    env.storage().persistent().set(
        &DataKey::PendingMerchantPull(subscriber.clone(), merchant.clone()),
        &pending,
    );

    SubscriptionBilled {
        subscriber: subscriber.clone(),
        merchant: merchant.clone(),
        amount,
        billed_at: pulled_at,
    }
    .publish(env);

    billing.next_billing_date = now.saturating_add(billing.billing_cycle);
    if billing.status == SubscriptionStatus::Trial {
        billing.status = SubscriptionStatus::Active;
        TrialConverted {
            subscriber: subscriber.clone(),
            merchant: merchant.clone(),
            converted_at: now,
        }
        .publish(env);
    } else if billing.status == SubscriptionStatus::PastDue {
        billing.status = SubscriptionStatus::Active;
        billing.dunning_start_timestamp = 0;
    }
    env.storage().persistent().set(&billing_key, &billing);
}

pub fn raise_dispute(env: &Env, subscriber: Address, merchant: Address, bond_amount: i128) {
    subscriber.require_auth();
    if bond_amount <= 0 {
        panic!("bond must be positive");
    }

    let billing_key = DataKey::BillingCycle(subscriber.clone(), merchant.clone());
    let mut billing: BillingCycleInfo = env
        .storage()
        .persistent()
        .get(&billing_key)
        .expect("no billing subscription");
    if billing.status == SubscriptionStatus::Disputed {
        panic!("already disputed");
    }

    let active_key = DataKey::ActiveDispute(subscriber.clone(), merchant.clone());
    if env.storage().persistent().has(&active_key) {
        panic!("active dispute");
    }

    let pending_key = DataKey::PendingMerchantPull(subscriber.clone(), merchant.clone());
    let pending: PendingMerchantPullInfo = env
        .storage()
        .persistent()
        .get(&pending_key)
        .expect("no pull to dispute");

    let now = env.ledger().timestamp();
    if now.saturating_sub(pending.pulled_at) > crate::DISPUTE_WINDOW_SEC {
        panic!("dispute window closed");
    }

    let contract = env.current_contract_address();
    let token_client = TokenClient::new(env, &pending.token);
    token_client.transfer(&subscriber, &contract, &bond_amount);

    let dispute_id = next_dispute_id(env);

    let record = DisputeRecord {
        dispute_id,
        subscriber: subscriber.clone(),
        merchant: merchant.clone(),
        disputed_amount: pending.amount,
        bond_amount,
        token: pending.token.clone(),
        raised_at: now,
        resolved: false,
    };
    env.storage()
        .persistent()
        .set(&DataKey::DisputeRecord(dispute_id), &record);
    env.storage()
        .persistent()
        .set(&active_key, &dispute_id);
    env.storage().persistent().remove(&pending_key);

    billing.status = SubscriptionStatus::Disputed;
    env.storage().persistent().set(&billing_key, &billing);

    DisputeRaised {
        dispute_id,
        subscriber: subscriber.clone(),
        merchant: merchant.clone(),
        disputed_amount: pending.amount,
        bond_amount,
        raised_at: now,
    }
    .publish(env);
}

pub fn resolve_dispute_for_user(
    env: &Env,
    subscriber: Address,
    merchant: Address,
    dispute_id: u64,
    juror_sigs: Vec<JurorSignature>,
) {
    resolve_dispute(env, &subscriber, &merchant, dispute_id, true, juror_sigs);
}

pub fn resolve_dispute_for_merchant(
    env: &Env,
    subscriber: Address,
    merchant: Address,
    dispute_id: u64,
    juror_sigs: Vec<JurorSignature>,
) {
    resolve_dispute(env, &subscriber, &merchant, dispute_id, false, juror_sigs);
}

fn resolve_dispute(
    env: &Env,
    subscriber: &Address,
    merchant: &Address,
    dispute_id: u64,
    user_wins: bool,
    juror_sigs: Vec<JurorSignature>,
) {
    verify_juror_threshold(env, dispute_id, user_wins, juror_sigs);

    let active_key = DataKey::ActiveDispute(subscriber.clone(), merchant.clone());
    let stored_id: u64 = env
        .storage()
        .persistent()
        .get(&active_key)
        .expect("no active dispute");
    if stored_id != dispute_id {
        panic!("dispute id mismatch");
    }

    let record_key = DataKey::DisputeRecord(dispute_id);
    let mut record: DisputeRecord = env
        .storage()
        .persistent()
        .get(&record_key)
        .expect("dispute record missing");
    if record.resolved {
        panic!("already resolved");
    }

    let contract = env.current_contract_address();
    let token_client = TokenClient::new(env, &record.token);
    let now = env.ledger().timestamp();

    let (paid_user, paid_merchant, bond_to, bond_amt) = if user_wins {
        let total_user = record
            .disputed_amount
            .saturating_add(record.bond_amount);
        if total_user > 0 {
            token_client.transfer(&contract, subscriber, &total_user);
        }
        (
            total_user,
            0i128,
            subscriber.clone(),
            record.bond_amount,
        )
    } else {
        if record.disputed_amount > 0 {
            token_client.transfer(&contract, merchant, &record.disputed_amount);
        }
        if record.bond_amount > 0 {
            token_client.transfer(&contract, merchant, &record.bond_amount);
        }
        (
            0i128,
            record.disputed_amount,
            merchant.clone(),
            record.bond_amount,
        )
    };

    record.resolved = true;
    env.storage().persistent().set(&record_key, &record);
    env.storage().persistent().remove(&active_key);

    let billing_key = DataKey::BillingCycle(subscriber.clone(), merchant.clone());
    if let Some(mut billing) = env.storage().persistent().get::<BillingCycleInfo>(&billing_key) {
        if user_wins {
            billing.status = SubscriptionStatus::Canceled;
            billing.next_billing_date = now;
        } else {
            billing.status = SubscriptionStatus::Active;
            billing.next_billing_date = now.saturating_add(billing.billing_cycle);
        }
        env.storage().persistent().set(&billing_key, &billing);
    }

    if user_wins {
        let sub_key = crate::subscription_key(subscriber, merchant);
        if crate::subscription_exists(env, &sub_key) {
            let sub = crate::get_subscription(env, &sub_key);
            let rate = sub.tier.rate_per_second;
            let mut total_flow: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::CurrentFlowRate(merchant.clone()))
                .unwrap_or(0);
            total_flow = total_flow.saturating_sub(rate);
            env.storage()
                .persistent()
                .set(&DataKey::CurrentFlowRate(merchant.clone()), &total_flow);
            crate::unregister_creator_support(env, merchant, subscriber);
            env.storage().persistent().remove(&sub_key);
            env.storage().temporary().remove(&sub_key);
        }
    }

    DisputeResolved {
        dispute_id,
        subscriber: subscriber.clone(),
        merchant: merchant.clone(),
        user_wins,
        refunded_to_user: paid_user,
        paid_to_merchant: paid_merchant,
        bond_destination: bond_to,
        bond_amount: bond_amt,
        resolved_at: now,
    }
    .publish(env);
}
