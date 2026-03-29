#![no_std]
#[cfg(test)]
extern crate std;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::{contract, contractevent, contractimpl, contracttype, vec, Address, Env, Vec};

// --- Constants ---
const MINIMUM_FLOW_DURATION: u64 = 86400;
const FREE_TRIAL_DURATION: u64 = 7 * 24 * 60 * 60;
const GRACE_PERIOD: u64 = 24 * 60 * 60; 
const GENESIS_NFT_ADDRESS: &str = "CAS3J7GYCCX7RRBHAHXDUY3OOWFMTIDDNVGCH6YOY7W7Y7G656H2HHMA";
const DISCOUNT_BPS: i128 = 2000; 
const SIX_MONTHS: u64 = 180 * 24 * 60 * 60;
const PRECISION_MULTIPLIER: i128 = 1_000_000_000;

// --- Helper: Charge Calculation ---
fn calculate_discounted_charge(start_time: u64, charge_start: u64, now: u64, base_rate: i128) -> i128 {
    if now <= charge_start {
        return 0;
    }

    let mut total_charge: i128 = 0;
    let mut current_t = charge_start;

    while current_t < now {
        let elapsed_since_start = current_t.saturating_sub(start_time);
        let periods = elapsed_since_start / SIX_MONTHS;
        let percent_discount = periods * 5;
        let discount = if percent_discount > 100 { 100 } else { percent_discount };

        let current_rate = base_rate * (100 - discount as i128) / 100;

        let next_boundary = start_time + (periods + 1) * SIX_MONTHS;
        let end_t = if now < next_boundary { now } else { next_boundary };

        let duration = (end_t - current_t) as i128;
        total_charge = total_charge.saturating_add(duration.saturating_mul(current_rate));

        current_t = end_t;
    }
    total_charge
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Subscription(Address, Address),
    TotalStreamed(Address, Address),
    CliffThreshold(Address),
    CreatorSubscribers(Address),
    CreatorMetadata(Address),
    ChannelPaused(Address),
    GiftsReceived(Address),
    CreatorSplit(Address),
    ContractAdmin,
    VerifiedCreator(Address),
    BlacklistedUser(Address, Address), // (creator, user_to_block)
    CreatorAudience(Address, Address), // (creator, beneficiary)
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Tier {
    pub rate_per_second: i128,
    pub trial_duration: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Subscription {
    pub token: Address,
    pub tier: Tier,
    pub balance: i128,
    pub last_collected: u64,
    pub start_time: u64,
    pub last_funds_exhausted: u64,
    pub creators: soroban_sdk::Vec<Address>,
    pub percentages: soroban_sdk::Vec<u32>,
    pub payer: Address,
    pub beneficiary: Address,
    pub accrued_remainder: i128, // Dust/fractional units that haven't been paid as tokens
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SplitPartition {
    pub partner: Address,
    pub percentage: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreatorStats {
    pub total_earned: i128,
    pub lifetime_fans: u64,
    pub active_fans: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreatorAudience {
    pub active_streams: u32,
    pub has_supported: bool,
}

// --- Events ---
#[contractevent]
pub struct TierChanged {
    #[topic] pub subscriber: Address,
    #[topic] pub creator: Address,
    pub old_rate: i128,
    pub new_rate: i128,
}

#[contractevent]
pub struct FreeToPaidTierActivated {
    #[topic] pub subscriber: Address,
    #[topic] pub creator: Address,
    pub rate_per_second: i128,
    pub activated_at: u64,
}

#[contractevent]
pub struct Subscribed {
    #[topic] pub subscriber: Address,
    #[topic] pub creator: Address,
    pub rate_per_second: i128,
}

#[contractevent]
pub struct Unsubscribed {
    #[topic] pub subscriber: Address,
    #[topic] pub creator: Address,
}

#[contractevent]
pub struct TipReceived {
    #[topic] pub user: Address,
    #[topic] pub creator: Address,
    #[topic] pub token: Address,
    pub amount: i128,
}

#[contractevent]
pub struct CreatorVerified {
    #[topic] pub creator: Address,
    #[topic] pub verified_by: Address,
}

#[contract]
pub struct SubStreamContract;

#[contractimpl]
impl SubStreamContract {
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().persistent().has(&DataKey::ContractAdmin) {
            panic!("already initialized");
        }
        env.storage().persistent().set(&DataKey::ContractAdmin, &admin);
    }

    pub fn verify_creator(env: Env, admin: Address, creator: Address) {
        admin.require_auth();
        let stored_admin: Address = env.storage().persistent().get(&DataKey::ContractAdmin).expect("not initialized");
        if admin != stored_admin { panic!("admin only"); }

        env.storage().persistent().set(&DataKey::VerifiedCreator(creator.clone()), &true);
        CreatorVerified { creator, verified_by: admin }.publish(&env);
    }

    pub fn is_creator_verified(env: Env, creator: Address) -> bool {
        env.storage().persistent().get(&DataKey::VerifiedCreator(creator)).unwrap_or(false)
    }

    pub fn subscribe(env: Env, subscriber: Address, creator: Address, token: Address, amount: i128, rate_per_second: i128) {
        Self::subscribe_gift(&env, subscriber.clone(), subscriber, creator, token, amount, rate_per_second);
    }

    pub fn subscribe_gift(env: &Env, payer: Address, beneficiary: Address, creator: Address, token: Address, amount: i128, rate_per_second: i128) {
        subscribe_core(env, &payer, &beneficiary, &creator, &token, amount, rate_per_second, vec![env, creator.clone()], vec![env, 100u32]);
    }

    pub fn is_subscribed(env: Env, subscriber: Address, creator: Address) -> bool {
        let key = subscription_key(&subscriber, &creator);
        if !subscription_exists(&env, &key) { return false; }
        
        let sub = get_subscription(&env, &key);
        if sub.tier.rate_per_second <= 0 { return false; }

        let trial_end = sub.start_time.saturating_add(sub.tier.trial_duration);
        let charge_start = if sub.last_collected > trial_end { sub.last_collected } else { trial_end };
        let now = env.ledger().timestamp();

        if now <= charge_start { return true; }

        // Use the discounted charge logic for consistent "is active" checks
        let potential_charge = calculate_discounted_charge(sub.start_time, charge_start, now, sub.tier.rate_per_second);

        #[cfg(test)]
        extern crate std as std2;
        #[cfg(test)]
        std2::eprintln!("IS_SUBSCRIBED DEBUG: start_time={} last_collected={} trial_end={} charge_start={} now={} balance={} potential_charge={}",
            sub.start_time, sub.last_collected, sub.start_time.saturating_add(sub.tier.trial_duration), charge_start, now, sub.balance, potential_charge);

        if sub.balance > potential_charge { return true; }

        // Grace period check
        if sub.last_funds_exhausted > 0 {
            let grace_period_end = sub.last_funds_exhausted.saturating_add(GRACE_PERIOD);
            if now <= grace_period_end { return true; }
        }
        false
    }

    pub fn collect(env: Env, subscriber: Address, creator: Address) {
        distribute_and_collect(&env, &subscriber, &creator, Some(&creator));
    }

    pub fn top_up(env: Env, subscriber: Address, stream_id: Address, amount: i128) {
        top_up_internal(&env, &subscriber, &stream_id, amount);
    }

    pub fn cancel(env: Env, subscriber: Address, creator: Address) {
        cancel_internal(&env, &subscriber, &creator);
    }

    pub fn tip(env: Env, user: Address, creator: Address, token: Address, amount: i128) {
        user.require_auth();
        if amount <= 0 || user == creator { panic!("invalid tip"); }
        let token_client = TokenClient::new(&env, &token);
        token_client.transfer(&user, &creator, &amount);
        TipReceived { user, creator, token, amount }.publish(&env);
    }

    pub fn subscribe_group(env: Env, payer: Address, channel_id: Address, token: Address, amount: i128, rate_per_second: i128, creators: soroban_sdk::Vec<Address>, percentages: soroban_sdk::Vec<u32>) {
        // Validate exactly 5 creators
        if creators.len() != 5 {
            panic!("group channel must contain exactly 5 creators");
        }
        // Validate percentages sum to 100
        let mut total_percentage: u32 = 0;
        for i in 0..percentages.len() {
            total_percentage += percentages.get(i).unwrap();
        }
        if total_percentage != 100 {
            panic!("percentages must sum to 100");
        }
        subscribe_core(&env, &payer, &payer, &channel_id, &token, amount, rate_per_second, creators, percentages);
    }

    pub fn collect_group(env: Env, subscriber: Address, channel_id: Address) {
        distribute_and_collect(&env, &subscriber, &channel_id, None);
    }

    pub fn cancel_group(env: Env, subscriber: Address, channel_id: Address) {
        cancel_internal(&env, &subscriber, &channel_id);
    }

    // --- Blacklist functionality for Issue #25 ---
    
    pub fn blacklist_user(env: Env, creator: Address, user_to_block: Address) {
        creator.require_auth();
        
        let blacklist_key = DataKey::BlacklistedUser(creator.clone(), user_to_block.clone());
        
        // Check if already blacklisted
        if env.storage().persistent().has(&blacklist_key) {
            panic!("user already blacklisted");
        }
        
        // Add to blacklist
        env.storage().persistent().set(&blacklist_key, &true);
        
        // Emit event
        UserBlacklisted { creator, user: user_to_block }.publish(&env);
    }
    
    pub fn unblacklist_user(env: Env, creator: Address, user_to_unblock: Address) {
        creator.require_auth();
        
        let blacklist_key = DataKey::BlacklistedUser(creator.clone(), user_to_unblock.clone());
        
        // Check if user is actually blacklisted
        if !env.storage().persistent().has(&blacklist_key) {
            panic!("user not blacklisted");
        }
        
        // Remove from blacklist
        env.storage().persistent().remove(&blacklist_key);
        
        // Emit event
        UserUnblacklisted { creator, user: user_to_unblock }.publish(&env);
    }
    
    pub fn is_user_blacklisted(env: Env, creator: Address, user: Address) -> bool {
        let blacklist_key = DataKey::BlacklistedUser(creator, user);
        env.storage().persistent().get(&blacklist_key).unwrap_or(false)
    }

    pub fn creator_stats(env: Env, creator: Address) -> CreatorStats {
        get_creator_stats(&env, &creator)
    }
}

// --- Internal Logic & Helpers ---

fn subscription_key(subscriber: &Address, stream_id: &Address) -> DataKey {
    DataKey::Subscription(subscriber.clone(), stream_id.clone())
}

fn subscription_exists(env: &Env, key: &DataKey) -> bool {
    env.storage().persistent().has(key) || env.storage().temporary().has(key)
}

fn get_subscription(env: &Env, key: &DataKey) -> Subscription {
    if let Some(sub) = env.storage().persistent().get(key) { sub }
    else { env.storage().temporary().get(key).expect("not found") }
}

fn set_subscription(env: &Env, key: &DataKey, sub: &Subscription) {
    if sub.balance > 0 {
        env.storage().persistent().set(key, sub);
        env.storage().temporary().remove(key);
    } else {
        env.storage().temporary().set(key, sub);
        env.storage().persistent().remove(key);
    }
}

fn default_creator_stats() -> CreatorStats {
    CreatorStats {
        total_earned: 0,
        lifetime_fans: 0,
        active_fans: 0,
    }
}

fn get_creator_stats(env: &Env, creator: &Address) -> CreatorStats {
    env.storage()
        .persistent()
        .get(&DataKey::CreatorMetadata(creator.clone()))
        .unwrap_or(default_creator_stats())
}

fn set_creator_stats(env: &Env, creator: &Address, stats: &CreatorStats) {
    env.storage()
        .persistent()
        .set(&DataKey::CreatorMetadata(creator.clone()), stats);
}

fn register_creator_support(env: &Env, creator: &Address, beneficiary: &Address) {
    let relationship_key = DataKey::CreatorAudience(creator.clone(), beneficiary.clone());
    let mut relationship: CreatorAudience = env
        .storage()
        .persistent()
        .get(&relationship_key)
        .unwrap_or(CreatorAudience {
            active_streams: 0,
            has_supported: false,
        });
    let mut stats = get_creator_stats(env, creator);

    if !relationship.has_supported {
        relationship.has_supported = true;
        stats.lifetime_fans = stats.lifetime_fans.saturating_add(1);
    }

    if relationship.active_streams == 0 {
        stats.active_fans = stats.active_fans.saturating_add(1);
    }

    relationship.active_streams = relationship.active_streams.saturating_add(1);
    env.storage().persistent().set(&relationship_key, &relationship);
    set_creator_stats(env, creator, &stats);
}

fn unregister_creator_support(env: &Env, creator: &Address, beneficiary: &Address) {
    let relationship_key = DataKey::CreatorAudience(creator.clone(), beneficiary.clone());
    let Some(mut relationship): Option<CreatorAudience> = env.storage().persistent().get(&relationship_key) else {
        return;
    };

    if relationship.active_streams == 0 {
        return;
    }

    relationship.active_streams -= 1;

    let mut stats = get_creator_stats(env, creator);
    if relationship.active_streams == 0 {
        stats.active_fans = stats.active_fans.saturating_sub(1);
    }

    env.storage().persistent().set(&relationship_key, &relationship);
    set_creator_stats(env, creator, &stats);
}

fn credit_creator_earnings(env: &Env, creator: &Address, amount: i128) {
    if amount <= 0 {
        return;
    }

    let mut stats = get_creator_stats(env, creator);
    stats.total_earned = stats.total_earned.saturating_add(amount);
    set_creator_stats(env, creator, &stats);
}

fn distribute_and_collect(env: &Env, beneficiary: &Address, stream_id: &Address, total_streamed_creator: Option<&Address>) -> i128 {
    let key = subscription_key(beneficiary, stream_id);
    let mut sub = get_subscription(env, &key);
    let now = env.ledger().timestamp();

    if now <= sub.last_collected { return 0; }

    let trial_end = sub.start_time.saturating_add(sub.tier.trial_duration);
    if !sub.free_to_paid_emitted && sub.tier.rate_per_second > 0 && now > trial_end {
        FreeToPaidTierActivated {
            subscriber: beneficiary.clone(),
            creator: stream_id.clone(),
            rate_per_second: sub.tier.rate_per_second,
            activated_at: now,
        }
        .publish(env);
        sub.free_to_paid_emitted = true;
    }

    if let Some(creator) = total_streamed_creator {
        if is_creator_paused(env, creator) {
            sub.last_collected = now;
            set_subscription(env, &key, &sub);
            return 0;
        }
    }

    let charge_start = if sub.last_collected > trial_end { sub.last_collected } else { trial_end };
    if now <= charge_start { return 0; }

    let amount_to_collect = calculate_discounted_charge(sub.start_time, charge_start, now, sub.tier.rate_per_second);
    
    // Check if grace period is active or expired
    if sub.balance <= 0 && sub.last_funds_exhausted > 0 {
        let grace_period_end = sub.last_funds_exhausted.saturating_add(GRACE_PERIOD);
        if now > grace_period_end { return 0; }
    }

    if amount_to_collect > sub.balance {
        if sub.last_funds_exhausted == 0 { sub.last_funds_exhausted = now; }
        // During grace period, we cap payout at available balance to prevent contract draining
    }

    let available_balance = sub.balance.max(0);
    let total_accrued = amount_to_collect.saturating_add(sub.accrued_remainder);
    let amount_to_payout_nano = total_accrued.min(available_balance.saturating_add(sub.accrued_remainder));
    let amount_to_payout_tokens = amount_to_payout_nano / PRECISION_MULTIPLIER;

    if amount_to_payout_tokens > 0 {
        let token_client = TokenClient::new(env, &sub.token);
        let creators_len = sub.creators.len();
        let mut remaining = amount_to_payout_tokens;

        for i in 0..creators_len {
            let creator = sub.creators.get(i).unwrap();
            let share = sub.percentages.get(i).unwrap() as i128;
            let payout = if i + 1 == creators_len { remaining } else { (amount_to_payout_tokens * share) / 100 };
            remaining -= payout;
            if payout > 0 {
                credit_creator_earnings(env, &creator, payout);
                token_client.transfer(&env.current_contract_address(), &creator, &payout);
            }
        }
    }

    sub.balance -= amount_to_collect;
    sub.accrued_remainder = total_accrued - (amount_to_payout_tokens * PRECISION_MULTIPLIER);
    sub.last_collected = now;
    set_subscription(env, &key, &sub);
    amount_to_collect
}

fn top_up_internal(env: &Env, beneficiary: &Address, stream_id: &Address, amount: i128) {
    let key = subscription_key(beneficiary, stream_id);
    let mut sub = get_subscription(env, &key);
    sub.payer.require_auth();

    let token_client = TokenClient::new(env, &sub.token);
    token_client.transfer(&sub.payer, &env.current_contract_address(), &amount);
    
    sub.balance += amount * PRECISION_MULTIPLIER;
    if sub.balance > 0 { sub.last_funds_exhausted = 0; }
    set_subscription(env, &key, &sub);
    distribute_and_collect(env, beneficiary, stream_id, None);
}

fn cancel_internal(env: &Env, beneficiary: &Address, stream_id: &Address) {
    let key = subscription_key(beneficiary, stream_id);
    let mut sub = get_subscription(env, &key);
    sub.payer.require_auth();

    if env.ledger().timestamp() < sub.start_time + MINIMUM_FLOW_DURATION { panic!("too early"); }

    distribute_and_collect(env, beneficiary, stream_id, None);
    sub = get_subscription(env, &key); // Refresh after collect

    if sub.balance > 0 {
        let token_client = TokenClient::new(env, &sub.token);
        let refund_amount = sub.balance / PRECISION_MULTIPLIER;
        if refund_amount > 0 {
            token_client.transfer(&env.current_contract_address(), &sub.payer, &refund_amount);
        }
    }
    for i in 0..sub.creators.len() {
        let creator = sub.creators.get(i).unwrap();
        unregister_creator_support(env, &creator, beneficiary);
    }
    env.storage().persistent().remove(&key);
    env.storage().temporary().remove(&key);
}

fn subscribe_core(env: &Env, payer: &Address, beneficiary: &Address, stream_id: &Address, token: &Address, amount: i128, rate: i128, creators: soroban_sdk::Vec<Address>, percentages: soroban_sdk::Vec<u32>) {
    payer.require_auth();
    let key = subscription_key(beneficiary, stream_id);
    if subscription_exists(env, &key) { panic!("exists"); }

    let token_client = TokenClient::new(env, token);
    token_client.transfer(payer, &env.current_contract_address(), &amount);

    let now = env.ledger().timestamp();
    let creators_for_stats = creators.clone();
    let sub = Subscription {
        token: token.clone(),
        tier: Tier { rate_per_second: rate, trial_duration: FREE_TRIAL_DURATION },
        balance: amount * PRECISION_MULTIPLIER,
        last_collected: now,
        start_time: now,
        last_funds_exhausted: 0,
        free_to_paid_emitted: false,
        creators,
        percentages,
        payer: payer.clone(),
        beneficiary: beneficiary.clone(),
        accrued_remainder: 0,
    };
    set_subscription(env, &key, &sub);
    for i in 0..creators_for_stats.len() {
        let creator = creators_for_stats.get(i).unwrap();
        register_creator_support(env, &creator, beneficiary);
    }
    Subscribed { subscriber: beneficiary.clone(), creator: stream_id.clone(), rate_per_second: rate }.publish(env);
}

fn is_creator_paused(env: &Env, creator: &Address) -> bool {
    env.storage().persistent().get(&DataKey::ChannelPaused(creator.clone())).unwrap_or(false)
}

#[cfg(test)]
mod test;
#[cfg(test)]
mod test_withdrawal_consistency;
#[cfg(test)]
mod test_tiny_streams;
