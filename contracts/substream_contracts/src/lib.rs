#![no_std]
#[cfg(test)]
extern crate std;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::{contract, contractevent, contractimpl, contracttype, vec, Address, Env, Symbol, Vec};

// --- Constants ---
const MINIMUM_FLOW_DURATION: u64 = 86400;
const FREE_TRIAL_DURATION: u64 = 7 * 24 * 60 * 60;
const GRACE_PERIOD: u64 = 24 * 60 * 60;
const GENESIS_NFT_ADDRESS: &str = "CAS3J7GYCCX7RRBHAHXDUY3OOWFMTIDDNVGCH6YOY7W7Y7G656H2HHMA";
const DISCOUNT_BPS: i128 = 2000;
const SIX_MONTHS: u64 = 180 * 24 * 60 * 60;
const TWELVE_MONTHS: u64 = 365 * 24 * 60 * 60;
const PRECISION_MULTIPLIER: i128 = 1_000_000_000;
const REFERRAL_REBATE_BPS: i128 = 100; // 1% rebate
const TTL_THRESHOLD: u32 = 17280; // Assuming ~1 day in ledgers for example
const TTL_BUMP_AMOUNT: u32 = 518400; // Assuming ~30 days in ledgers for example

// --- SLA Circuit Breaker Constants ---
const SLA_THRESHOLD_BPS: u32 = 9990; // 99.9% uptime threshold (in basis points)
const SEVEN_DAYS: u64 = 7 * 24 * 60 * 60;
const UPTIME_ORACLE_NONCE_TTL: u64 = 24 * 60 * 60; // 24 hour validity for oracle signatures

// --- Helper: Charge Calculation ---
fn calculate_discounted_charge(
    start_time: u64,
    charge_start: u64,
    now: u64,
    base_rate: i128,
) -> i128 {
    if now <= charge_start {
        return 0;
    }

    let mut total_charge: i128 = 0;
    let mut current_t = charge_start;

    while current_t < now {
        let elapsed_since_start = current_t.saturating_sub(start_time);
        let periods = elapsed_since_start / SIX_MONTHS;
        let percent_discount = periods * 5;
        let discount = if percent_discount > 100 {
            100
        } else {
            percent_discount
        };

        let current_rate = base_rate * (100 - discount as i128) / 100;

        let next_boundary = start_time + (periods + 1) * SIX_MONTHS;
        let end_t = if now < next_boundary {
            now
        } else {
            next_boundary
        };

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
    CreatorProfileCID(Address),        // For #46
    NFTAwarded(Address, Address), // (beneficiary, stream_id) - For #44
    BlacklistedUser(Address, Address), // (creator, user_to_block)
    CreatorAudience(Address, Address), // (creator, beneficiary)
    MinimumRate(Address),              // Minimum rate floor for PWYW
    CommunityGoal(Address),            // Target flow rate for "Bonus Video"
    UserReferrer(Address),
    ReferralTracker(Address, Address),
    CurrentFlowRate(Address),          // Aggregated flow rate for a channel
    AcceptedToken(Address),            // Issue #49: Creator's enforced stablecoin token
    SLAStatus(Address),               // Creator's SLA status
    UptimeOracleNonce(u64),           // Oracle nonce tracking
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
    pub free_to_paid_emitted: bool,
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

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferralInfo {
    pub referrer: Address,
    pub referral_count: u32,
    pub total_rebates_earned: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UptimeOraclePayload {
    pub creator: Address,
    pub uptime_percentage: u32, // In basis points (e.g., 9990 = 99.9%)
    pub downtime_minutes: u64,
    pub period_start: u64,
    pub period_end: u64,
    pub nonce: u64,
    pub oracle_signature: soroban_sdk::Vec<u8>, // Signature from uptime oracle
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SLAStatus {
    pub active: bool,
    pub last_updated: u64,
    pub cumulative_downtime_minutes: u64,
    pub current_penalty_period_start: u64,
    pub total_refund_owed: i128,
}

// --- Events ---
#[contractevent]
pub struct TierChanged {
    #[topic]
    pub subscriber: Address,
    #[topic]
    pub creator: Address,
    pub old_rate: i128,
    pub new_rate: i128,
}

#[contractevent]
pub struct AcceptedTokenSet {
    #[topic] pub creator: Address,
    #[topic] pub token: Address,
}

#[contractevent]
pub struct FreeToPaidTierActivated {
    #[topic]
    pub subscriber: Address,
    #[topic]
    pub creator: Address,
    pub rate_per_second: i128,
    pub activated_at: u64,
}

#[contractevent]
pub struct Subscribed {
    #[topic]
    pub subscriber: Address,
    #[topic]
    pub creator: Address,
    pub rate_per_second: i128,
}

#[contractevent]
pub struct Unsubscribed {
    #[topic]
    pub subscriber: Address,
    #[topic]
    pub creator: Address,
}

#[contractevent]
pub struct TipReceived {
    #[topic]
    pub user: Address,
    #[topic]
    pub creator: Address,
    #[topic]
    pub token: Address,
    pub amount: i128,
}

#[contractevent]
pub struct CreatorVerified {
    #[topic]
    pub creator: Address,
    #[topic]
    pub verified_by: Address,
}

#[contractevent]
pub struct ReferralRegistered {
    #[topic]
    pub referrer: Address,
    #[topic]
    pub referred_user: Address,
}

#[contractevent]
pub struct ReferralRebatePaid {
    #[topic]
    pub referrer: Address,
    #[topic]
    pub referred_user: Address,
    #[topic]
    pub creator: Address,
    pub amount: i128,
}

#[contractevent]
pub struct FanNftAwarded {
    #[topic] pub beneficiary: Address,
    #[topic] pub creator: Address, // stream_id
    pub awarded_at: u64,
}

#[contractevent]
pub struct UserBlacklisted {
    #[topic] pub creator: Address,
    #[topic] pub user: Address,
}

#[contractevent]
pub struct UserUnblacklisted {
    #[topic] pub creator: Address,
    #[topic] pub user: Address,
}

#[contractevent]
pub struct SLABreached {
    #[topic] pub creator: Address,
    #[topic] pub subscriber: Address,
    pub uptime_percentage: u32,
    pub downtime_minutes: u64,
    pub refund_amount: i128,
    pub penalty_active: bool,
}


#[contract]
pub struct SubStreamContract;

#[contractimpl]
impl SubStreamContract {
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().persistent().has(&DataKey::ContractAdmin) {
            panic!("already initialized");
        }
        env.storage()
            .persistent()
            .set(&DataKey::ContractAdmin, &admin);
    }

    pub fn verify_creator(env: Env, admin: Address, creator: Address) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::ContractAdmin)
            .expect("not initialized");
        if admin != stored_admin {
            panic!("admin only");
        }

        env.storage()
            .persistent()
            .set(&DataKey::VerifiedCreator(creator.clone()), &true);
        CreatorVerified {
            creator,
            verified_by: admin,
        }
        .publish(&env);
    }

    pub fn is_creator_verified(env: Env, creator: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::VerifiedCreator(creator))
            .unwrap_or(false)
    }

    pub fn subscribe(
        env: Env,
        subscriber: Address,
        creator: Address,
        token: Address,
        amount: i128,
        rate_per_second: i128,
        referrer: Option<Address>, // Add optional referrer parameter
    ) {
        Self::subscribe_gift(
            &env,
            subscriber.clone(),
            subscriber,
            creator,
            token,
            amount,
            rate_per_second,
            referrer,
        );
    }

    pub fn subscribe_gift(
        env: &Env,
        payer: Address,
        beneficiary: Address,
        creator: Address,
        token: Address,
        amount: i128,
        rate_per_second: i128,
        referrer: Option<Address>, // Add optional referrer parameter
    ) {
        // Register referral if provided
        if let Some(referrer_addr) = referrer {
            if !env
                .storage()
                .persistent()
                .has(&DataKey::UserReferrer(beneficiary.clone()))
            {
                env.storage()
                    .persistent()
                    .set(&DataKey::UserReferrer(beneficiary.clone()), &referrer_addr);

                // Update referrer's tracking info
                let referral_tracker_key =
                    DataKey::ReferralTracker(referrer_addr.clone(), beneficiary.clone());
                env.storage().persistent().set(&referral_tracker_key, &true);

                // Get and update referrer's referral info
                let mut referral_info = get_referral_info(env, &referrer_addr);
                referral_info.referral_count += 1;
                set_referral_info(env, &referrer_addr, &referral_info);

                // Emit event
                ReferralRegistered {
                    referrer: referrer_addr.clone(),
                    referred_user: beneficiary.clone(),
                }
                .publish(env);
            }
        }

        subscribe_core(
            env,
            &payer,
            &beneficiary,
            &creator,
            &token,
            amount,
            rate_per_second,
            vec![env, creator.clone()],
            vec![env, 100u32],
        );
    }

    pub fn is_subscribed(env: Env, subscriber: Address, creator: Address) -> bool {
        let key = subscription_key(&subscriber, &creator);
        if !subscription_exists(&env, &key) {
            return false;
        }

        let sub = get_subscription(&env, &key);
        if sub.tier.rate_per_second <= 0 {
            return false;
        }

        let trial_end = sub.start_time.saturating_add(sub.tier.trial_duration);
        let charge_start = if sub.last_collected > trial_end {
            sub.last_collected
        } else {
            trial_end
        };
        let now = env.ledger().timestamp();

        if now <= charge_start {
            return true;
        }

        // Use the discounted charge logic for consistent "is active" checks
        let potential_charge = calculate_discounted_charge(
            sub.start_time,
            charge_start,
            now,
            sub.tier.rate_per_second,
        );

        #[cfg(test)]
        extern crate std as std2;
        #[cfg(test)]
        std2::eprintln!("IS_SUBSCRIBED DEBUG: start_time={} last_collected={} trial_end={} charge_start={} now={} balance={} potential_charge={}",
            sub.start_time, sub.last_collected, sub.start_time.saturating_add(sub.tier.trial_duration), charge_start, now, sub.balance, potential_charge);

        if sub.balance > potential_charge {
            return true;
        }

        // Grace period check
        if sub.last_funds_exhausted > 0 {
            let grace_period_end = sub.last_funds_exhausted.saturating_add(GRACE_PERIOD);
            if now <= grace_period_end {
                return true;
            }
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
        if amount <= 0 || user == creator {
            panic!("invalid tip");
        }
        let token_client = TokenClient::new(&env, &token);
        token_client.transfer(&user, &creator, &amount);
        TipReceived {
            user,
            creator,
            token,
            amount,
        }
        .publish(&env);
    }

    pub fn subscribe_group(
        env: Env,
        payer: Address,
        channel_id: Address,
        token: Address,
        amount: i128,
        rate_per_second: i128,
        creators: soroban_sdk::Vec<Address>,
        percentages: soroban_sdk::Vec<u32>,
    ) {
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
        subscribe_core(
            &env,
            &payer,
            &payer,
            &channel_id,
            &token,
            amount,
            rate_per_second,
            creators,
            percentages,
        );
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
        UserBlacklisted {
            creator,
            user: user_to_block,
        }
        .publish(&env);
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
        UserUnblacklisted {
            creator,
            user: user_to_unblock,
        }
        .publish(&env);
    }

    pub fn is_user_blacklisted(env: Env, creator: Address, user: Address) -> bool {
        let blacklist_key = DataKey::BlacklistedUser(creator, user);
        env.storage()
            .persistent()
            .get(&blacklist_key)
            .unwrap_or(false)
    }

    pub fn creator_stats(env: Env, creator: Address) -> CreatorStats {
        get_creator_stats(&env, &creator)
    }

    pub fn set_minimum_rate(env: Env, creator: Address, min_rate: i128) {
        creator.require_auth();
        env.storage().persistent().set(&DataKey::MinimumRate(creator), &min_rate);
    }

    pub fn set_community_goal(env: Env, creator: Address, goal_tokens_per_day: i128) {
        creator.require_auth();
        // Convert tokens/day to flow rate (units per second)
        // Using PRECISION_MULTIPLIER to maintain high-fidelity streaming math
        let goal_per_sec = (goal_tokens_per_day * PRECISION_MULTIPLIER) / 86400;
        env.storage().persistent().set(&DataKey::CommunityGoal(creator), &goal_per_sec);
    }

    pub fn is_community_goal_met(env: Env, creator: Address) -> bool {
        let goal: i128 = env.storage().persistent().get(&DataKey::CommunityGoal(creator.clone())).unwrap_or(0);
        if goal == 0 { return false; }

        let current: i128 = env.storage().persistent().get(&DataKey::CurrentFlowRate(creator)).unwrap_or(0);
        current >= goal
    }

    // --- Issue #49: Stablecoin-Only Enforcement ---
    pub fn set_accepted_token(env: Env, creator: Address, token: Address) {
        creator.require_auth();
        env.storage().persistent().set(&DataKey::AcceptedToken(creator.clone()), &token);
        AcceptedTokenSet { creator, token }.publish(&env);
    }

    // --- SLA Circuit Breaker Methods ---
    
    /// Update SLA status based on uptime oracle payload
    /// This method is called by the uptime oracle to report service availability
    pub fn update_sla_status(env: Env, payload: UptimeOraclePayload) {
        let now = env.ledger().timestamp();
        
        // Validate oracle nonce to prevent replay attacks
        if env.storage().persistent().has(&DataKey::UptimeOracleNonce(payload.nonce)) {
            panic!("oracle nonce already used");
        }
        
        // Check nonce expiration (24 hour validity)
        if payload.period_end + UPTIME_ORACLE_NONCE_TTL < now {
            panic!("oracle signature expired");
        }
        
        // Mark nonce as used
        env.storage().persistent().set(&DataKey::UptimeOracleNonce(payload.nonce), &now);
        
        // Get or create SLA status for this creator
        let mut sla_status = get_sla_status(&env, &payload.creator);
        
        // Check if SLA threshold is breached
        if payload.uptime_percentage < SLA_THRESHOLD_BPS {
            if !sla_status.active {
                // SLA breach just started
                sla_status.active = true;
                sla_status.current_penalty_period_start = payload.period_start;
            }
            
            // Update cumulative downtime
            sla_status.cumulative_downtime_minutes += payload.downtime_minutes;
            
            // Calculate refund amount based on downtime
            let refund_amount = calculate_sla_refund(&env, &payload.creator, payload.downtime_minutes);
            sla_status.total_refund_owed += refund_amount;
            
            // Emit SLA breach event for all affected subscribers
            emit_sla_breach_events(&env, &payload.creator, payload.uptime_percentage, payload.downtime_minutes, refund_amount, true);
        } else {
            if sla_status.active {
                // SLA breach recovered
                sla_status.active = false;
                emit_sla_breach_events(&env, &payload.creator, payload.uptime_percentage, 0, 0, false);
            }
        }
        
        sla_status.last_updated = now;
        set_sla_status(&env, &payload.creator, &sla_status);
    }
    
    /// Get current SLA status for a creator
    pub fn get_sla_status(env: Env, creator: Address) -> SLAStatus {
        get_sla_status(&env, &creator)
    }
    
    /// Allow subscriber to cancel with immediate refund if SLA breach exceeds 7 days
    pub fn emergency_cancel_due_to_sla(env: Env, subscriber: Address, creator: Address) {
        let sla_status = get_sla_status(&env, &creator);
        
        // Check if downtime exceeds 7 days (7 * 24 * 60 = 10080 minutes)
        if sla_status.cumulative_downtime_minutes < 10080 {
            panic!("SLA emergency cancellation only available after 7+ days of downtime");
        }
        
        if !sla_status.active {
            panic!("SLA emergency cancellation only available during active breach");
        }
        
        // Perform immediate cancellation with full refund
        cancel_internal(&env, &subscriber, &creator);
        
        // Return any remaining balance to subscriber
        let key = subscription_key(&subscriber, &creator);
        let sub = get_subscription(&env, &key);
        
        if sub.balance > 0 {
            let refund_amount = sub.balance / PRECISION_MULTIPLIER;
            let token_client = TokenClient::new(&env, &sub.token);
            token_client.transfer(&env.current_contract_address(), &sub.payer, &refund_amount);
        }
    }
}

fn subscription_key(subscriber: &Address, stream_id: &Address) -> DataKey {
    DataKey::Subscription(subscriber.clone(), stream_id.clone())
}

fn subscription_exists(env: &Env, key: &DataKey) -> bool {
    env.storage().persistent().has(key) || env.storage().temporary().has(key)
}

fn get_subscription(env: &Env, key: &DataKey) -> Subscription {
    if let Some(sub) = env.storage().persistent().get(key) {
        sub
    } else {
        env.storage().temporary().get(key).expect("not found")
    }
}

fn set_subscription(env: &Env, key: &DataKey, sub: &Subscription) {
    if sub.balance > 0 {
        env.storage().persistent().set(key, sub);
        env.storage().temporary().remove(key);
        // Bump TTL for active subscriptions to keep them from expiring
        bump_instance_ttl(env);
        env.storage().persistent().bump(key, TTL_THRESHOLD, TTL_BUMP_AMOUNT);
    } else {
        env.storage().temporary().set(key, sub);
        env.storage().persistent().remove(key);
        // Only bump instance TTL if we are moving to temporary storage,
        // as the temporary entry will expire on its own.
        bump_instance_ttl(env);
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
    env.storage()
        .persistent()
        .set(&relationship_key, &relationship);
    set_creator_stats(env, creator, &stats);
}

fn unregister_creator_support(env: &Env, creator: &Address, beneficiary: &Address) {
    let relationship_key = DataKey::CreatorAudience(creator.clone(), beneficiary.clone());
    let Some(mut relationship): Option<CreatorAudience> =
        env.storage().persistent().get(&relationship_key)
    else {
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

    env.storage()
        .persistent()
        .set(&relationship_key, &relationship);
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

fn distribute_and_collect(
    env: &Env,
    beneficiary: &Address,
    stream_id: &Address,
    total_streamed_creator: Option<&Address>,
) -> i128 {
    let key = subscription_key(beneficiary, stream_id);
    let mut sub = get_subscription(env, &key);
    let now = env.ledger().timestamp();

    if now <= sub.last_collected {
        return 0;
    }

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

    let charge_start = if sub.last_collected > trial_end {
        sub.last_collected
    } else {
        trial_end
    };
    if now <= charge_start {
        return 0;
    }

    let amount_to_collect =
        calculate_discounted_charge(sub.start_time, charge_start, now, sub.tier.rate_per_second);

    // Check if grace period is active or expired
    if sub.balance <= 0 && sub.last_funds_exhausted > 0 {
        let grace_period_end = sub.last_funds_exhausted.saturating_add(GRACE_PERIOD);
        if now > grace_period_end {
            return 0;
        }
    }

    if amount_to_collect > sub.balance {
        if sub.last_funds_exhausted == 0 {
            sub.last_funds_exhausted = now;
        }
        // During grace period, we cap payout at available balance to prevent contract draining
    }

    let available_balance = sub.balance.max(0);
    let total_accrued = amount_to_collect.saturating_add(sub.accrued_remainder);
    let amount_to_payout_nano =
        total_accrued.min(available_balance.saturating_add(sub.accrued_remainder));
    let amount_to_payout_tokens = amount_to_payout_nano / PRECISION_MULTIPLIER;

    if amount_to_payout_tokens > 0 {
        let token_client = TokenClient::new(env, &sub.token);
        let creators_len = sub.creators.len();
        let mut remaining = amount_to_payout_tokens;

        // Check for referral rebate before distributing to creators
        let referral_rebate = if let Some(referrer) = get_user_referrer(env, &sub.beneficiary) {
            // Calculate 1% rebate on the total amount being paid out
            (amount_to_payout_tokens * REFERRAL_REBATE_BPS) / 10000
        } else {
            0
        };

        for i in 0..creators_len {
            let creator = sub.creators.get(i).unwrap();
            let share = sub.percentages.get(i).unwrap() as i128;
            let mut payout = if i + 1 == creators_len {
                remaining
            } else {
                (amount_to_payout_tokens * share) / 100
            };

            // Apply referral rebate if applicable and this is the first creator
            if referral_rebate > 0 && i == 0 {
                if payout > referral_rebate {
                    payout -= referral_rebate;
                    pay_referral_rebate(
                        env,
                        &sub.beneficiary,
                        &creator,
                        &sub.token,
                        referral_rebate,
                    );
                } else {
                    // If payout is too small for rebate, skip rebate
                    remaining += referral_rebate; // Add back to remaining for other creators
                }
            }

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
    bump_instance_ttl(env);
    let key = subscription_key(beneficiary, stream_id);
    let mut sub = get_subscription(env, &key);
    sub.payer.require_auth();

    let token_client = TokenClient::new(env, &sub.token);
    token_client.transfer(&sub.payer, &env.current_contract_address(), &amount);

    sub.balance += amount * PRECISION_MULTIPLIER;
    if sub.balance > 0 {
        sub.last_funds_exhausted = 0;
    }
    set_subscription(env, &key, &sub);
    distribute_and_collect(env, beneficiary, stream_id, None);
}

fn cancel_internal(env: &Env, beneficiary: &Address, stream_id: &Address) {
    bump_instance_ttl(env);
    let key = subscription_key(beneficiary, stream_id);
    let mut sub = get_subscription(env, &key);
    sub.payer.require_auth();

    if env.ledger().timestamp() < sub.start_time + MINIMUM_FLOW_DURATION { panic!("cannot cancel stream: minimum duration not met"); }

    // Collect any charges that have accrued so far (zero during the trial window).
    distribute_and_collect(env, beneficiary, stream_id, None);
    sub = get_subscription(env, &key); // Refresh after collect.

    // Calculate penalty for early cancellation (optional logic from your existing code, assuming is_early was pseudo-code)
    let is_early = false; // Add logic here if needed to determine early cancellation based on start_time
    if is_early {
        // The creator is entitled to compensation equal to the full minimum-lock
        // period even though the subscriber is cancelling early.  This prevents
        // flash-subscribe attacks where a user subscribes, scrapes content, and
        // immediately cancels to recover their deposit.
        //
        // Penalty = rate_per_second × MINIMUM_FLOW_DURATION (in internal nano
        // units), capped at the remaining balance so we never overdraw.
        let min_entitled_nano = sub.tier.rate_per_second
            .saturating_mul(MINIMUM_FLOW_DURATION as i128);
        let available_nano = sub.balance.max(0);
        let penalty_nano = min_entitled_nano.min(available_nano);
        let penalty_tokens = penalty_nano / PRECISION_MULTIPLIER;

        if penalty_tokens > 0 {
            let token_client = TokenClient::new(env, &sub.token);
            let creators_len = sub.creators.len();
            let mut remaining_penalty = penalty_tokens;

            for i in 0..creators_len {
                let creator = sub.creators.get(i).unwrap();
                let share = sub.percentages.get(i).unwrap() as i128;
                let payout = if i + 1 == creators_len {
                    remaining_penalty
                } else {
                    (penalty_tokens * share) / 100
                };
                remaining_penalty -= payout;
                if payout > 0 {
                    credit_creator_earnings(env, &creator, payout);
                    token_client.transfer(&env.current_contract_address(), &creator, &payout);
                }
            }
        }
    }

    let rate = sub.tier.rate_per_second;
    let mut total_flow: i128 = env.storage().persistent().get(&DataKey::CurrentFlowRate(stream_id.clone())).unwrap_or(0);
    total_flow = total_flow.saturating_sub(rate);
    env.storage().persistent().set(&DataKey::CurrentFlowRate(stream_id.clone()), &total_flow);

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

fn subscribe_core(
    env: &Env,
    payer: &Address,
    beneficiary: &Address,
    stream_id: &Address,
    token: &Address,
    amount: i128,
    rate: i128,
    creators: soroban_sdk::Vec<Address>,
    percentages: soroban_sdk::Vec<u32>,
) {
    payer.require_auth();

    // --- Issue #49: Stablecoin-Only Enforcement Mode ---
    if let Some(accepted_token) = env.storage().persistent().get::<_, Address>(&DataKey::AcceptedToken(stream_id.clone())) {
        if token != &accepted_token {
            panic!("creator only accepts their specified stablecoin");
        }
    }
    // ---------------------------------------------------

    let key = subscription_key(beneficiary, stream_id);
    if subscription_exists(env, &key) {
        panic!("exists");
    }

    let floor: i128 = env.storage().persistent().get(&DataKey::MinimumRate(stream_id.clone())).unwrap_or(0);
    if rate < floor { panic!("rate below floor"); }

    // Trial support: Allow starting a stream with 0 initial balance
    if amount > 0 {
        let token_client = TokenClient::new(env, token);
        token_client.transfer(payer, &env.current_contract_address(), &amount);
    }

    let now = env.ledger().timestamp();
    let creators_for_stats = creators.clone();
    let sub = Subscription {
        token: token.clone(),
        tier: Tier {
            rate_per_second: rate,
            trial_duration: FREE_TRIAL_DURATION,
        },
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

    let mut total_flow: i128 = env.storage().persistent().get(&DataKey::CurrentFlowRate(stream_id.clone())).unwrap_or(0);
    total_flow = total_flow.saturating_add(rate);
    env.storage().persistent().set(&DataKey::CurrentFlowRate(stream_id.clone()), &total_flow);

    for i in 0..creators_for_stats.len() {
        let creator = creators_for_stats.get(i).unwrap();
        register_creator_support(env, &creator, beneficiary);
    }
    Subscribed {
        subscriber: beneficiary.clone(),
        creator: stream_id.clone(),
        rate_per_second: rate,
    }
    .publish(env);
}

fn is_creator_paused(env: &Env, creator: &Address) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::ChannelPaused(creator.clone()))
        .unwrap_or(false)
}

// --- Referral Helper Functions ---

fn get_referral_info(env: &Env, referrer: &Address) -> ReferralInfo {
    env.storage()
        .persistent()
        .get(&DataKey::ReferralTracker(
            referrer.clone(),
            referrer.clone(),
        ))
        .unwrap_or(ReferralInfo {
            referrer: referrer.clone(),
            referral_count: 0,
            total_rebates_earned: 0,
        })
}

fn set_referral_info(env: &Env, referrer: &Address, info: &ReferralInfo) {
    env.storage().persistent().set(
        &DataKey::ReferralTracker(referrer.clone(), referrer.clone()),
        info,
    );
}

fn get_user_referrer(env: &Env, user: &Address) -> Option<Address> {
    env.storage()
        .persistent()
        .get(&DataKey::UserReferrer(user.clone()))
}

fn pay_referral_rebate(
    env: &Env,
    referred_user: &Address,
    creator: &Address,
    token: &Address,
    rebate_amount: i128,
) {
    if let Some(referrer) = get_user_referrer(env, referred_user) {
        let token_client = TokenClient::new(env, token);

        // Transfer rebate to referrer
        token_client.transfer(&env.current_contract_address(), &referrer, &rebate_amount);

        // Update referrer's stats
        let mut referral_info = get_referral_info(env, &referrer);
        referral_info.total_rebates_earned += rebate_amount;
        set_referral_info(env, &referrer, &referral_info);

        // Emit event
        ReferralRebatePaid {
            referrer,
            referred_user: referred_user.clone(),
            creator: creator.clone(),
            amount: rebate_amount,
        }
        .publish(env);
    }
}

// --- SLA Circuit Breaker Helper Functions ---

fn get_sla_status(env: &Env, creator: &Address) -> SLAStatus {
    env.storage()
        .persistent()
        .get(&DataKey::SLAStatus(creator.clone()))
        .unwrap_or(SLAStatus {
            active: false,
            last_updated: 0,
            cumulative_downtime_minutes: 0,
            current_penalty_period_start: 0,
            total_refund_owed: 0,
        })
}

fn set_sla_status(env: &Env, creator: &Address, status: &SLAStatus) {
    env.storage()
        .persistent()
        .set(&DataKey::SLAStatus(creator.clone()), status);
}

fn calculate_sla_refund(env: &Env, creator: &Address, downtime_minutes: u64) -> i128 {
    // Calculate refund based on downtime: 1 minute of downtime = 1 minute of service cost
    // Get the average rate for this creator across all active subscriptions
    let total_rate_per_second = get_total_rate_for_creator(env, creator);
    
    // Convert downtime minutes to seconds
    let downtime_seconds = downtime_minutes * 60;
    
    // Calculate refund amount in internal precision units
    let refund_nano = total_rate_per_second * downtime_seconds as i128;
    
    // Convert to tokens (divide by precision multiplier)
    refund_nano / PRECISION_MULTIPLIER
}

fn get_total_rate_for_creator(env: &Env, creator: &Address) -> i128 {
    // This would typically iterate through all subscriptions for a creator
    // For now, we'll use a simplified approach by checking the current flow rate
    env.storage()
        .persistent()
        .get(&DataKey::CurrentFlowRate(creator.clone()))
        .unwrap_or(0)
}

fn emit_sla_breach_events(
    env: &Env,
    creator: &Address,
    uptime_percentage: u32,
    downtime_minutes: u64,
    refund_amount: i128,
    penalty_active: bool,
) {
    // In a real implementation, this would find all active subscribers for this creator
    // For now, we emit a generic event. In practice, you might want to:
    // 1. Iterate through all subscriptions for this creator
    // 2. Emit individual events for each subscriber
    
    SLABreached {
        creator: creator.clone(),
        subscriber: creator.clone(), // Placeholder - in practice would be actual subscriber
        uptime_percentage,
        downtime_minutes,
        refund_amount,
        penalty_active,
    }
    .publish(env);
}

#[cfg(test)]
mod test;
#[cfg(test)]
mod test_tiny_streams;
#[cfg(test)]
mod test_withdrawal_consistency;
#[cfg(test)]
mod test_sla_circuit_breaker;