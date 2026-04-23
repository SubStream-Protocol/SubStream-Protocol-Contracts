#![no_std]
<<<<<<< feature/advanced-subscription-engine-bundle
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::{contract, contractevent, contractimpl, contracttype, vec, Address, Bytes, Env, IntoVal, Vec};
=======
#[cfg(test)]
extern crate std;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::{contract, contractevent, contractimpl, contracttype, vec, Address, Env, Symbol, Vec};
>>>>>>> main

// --- Constants ---
const MINIMUM_FLOW_DURATION: u64 = 86400;
const FREE_TRIAL_DURATION: u64 = 7 * 24 * 60 * 60;
const MAX_GRACE_PERIOD: u64 = 7 * 24 * 60 * 60;
const GENESIS_NFT_ADDRESS: &str = "CAS3J7GYCCX7RRBHAHXDUY3OOWFMTIDDNVGCH6YOY7W7Y7G656H2HHMA";
const DISCOUNT_BPS: i128 = 2000;
const SIX_MONTHS: u64 = 180 * 24 * 60 * 60;
const TWELVE_MONTHS: u64 = 365 * 24 * 60 * 60;
const PRECISION_MULTIPLIER: i128 = 1_000_000_000;
const REFERRAL_REBATE_BPS: i128 = 100; // 1% rebate
const TTL_THRESHOLD: u32 = 17280; // Assuming ~1 day in ledgers for example
const TTL_BUMP_AMOUNT: u32 = 518400; // Assuming ~30 days in ledgers for example

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
<<<<<<< feature/advanced-subscription-engine-bundle
    Stream(Address, Address),        // (subscriber, creator)
    TotalStreamed(Address, Address), // (subscriber, creator) - cumulative tokens streamed
    CliffThreshold(Address),         // creator -> threshold amount for access
    CreatorSubscribers(Address),     // creator -> Vec<subscriber>
    CreatorMetadata(Address),        // creator -> IPFS CID bytes
    ChannelPaused(Address),          // creator -> bool
    Escrow(Address, Address),        // (subscriber, merchant)
    Nullifier(Bytes),                // ZK nullifier tracking
    YieldConfig(Address),            // merchant -> YieldConfig
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowVault {
    pub token: Address,
    pub merchant: Address,
    pub subscriber: Address,
    pub total_amount: i128,
    pub vested_amount: i128,
    pub start_time: u64,
    pub end_time: u64,
    pub last_drip: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YieldConfig {
    pub target_protocol: Address,
    pub user_share_bps: u32,     // bps (0-10000)
    pub merchant_share_bps: u32,
    pub dao_share_bps: u32,
=======
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
    PlanRegistry(Address),             // Merchant's pricing plans registry
    TrialUsed(Address, Address),      // (user, merchant) - Prevent trial abuse
    BillingCycle(Address, Address),    // (subscriber, merchant) - Billing cycle info
>>>>>>> main
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
pub struct Plan {
    pub plan_id: u32,
    pub name: soroban_sdk::String,
    pub billing_amount: i128,
    pub billing_cycle: u64, // Duration in seconds
    pub has_trial: bool,
    pub trial_duration: u64,
    pub is_active: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SubscriptionStatus {
    Active,
    PastDue,
    Canceled,
    Trial,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BillingCycleInfo {
    pub next_billing_date: u64,
    pub dunning_start_timestamp: u64,
    pub status: SubscriptionStatus,
    pub billing_amount: i128,
    pub billing_cycle: u64,
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
<<<<<<< feature/advanced-subscription-engine-bundle
pub struct CrossAssetBilled {
    #[topic]
    pub subscriber: Address,
    #[topic]
    pub merchant: Address,
    pub asset_in: Address,
    pub asset_out: Address,
    pub amount_in: i128,
    pub amount_out: i128,
}

#[contractevent]
pub struct AnnualEscrowLocked {
    #[topic]
    pub subscriber: Address,
    #[topic]
    pub merchant: Address,
    pub amount: i128,
    pub duration_months: u32,
}

#[contractevent]
pub struct YieldHarvested {
    #[topic]
    pub merchant: Address,
    pub profit: i128,
    pub user_distributed: i128,
    pub merchant_distributed: i128,
    pub dao_distributed: i128,
}

#[contractevent]
pub struct AccessGranted {
    #[topic]
    pub merchant: Address,
    #[topic]
    pub nullifier: Bytes,
}

#[contract]
pub struct SubStreamContract;
=======
pub struct CreatorVerified {
    #[topic]
    pub creator: Address,
    #[topic]
    pub verified_by: Address,
}
>>>>>>> main

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
pub struct SubscriptionBilled {
    #[topic] pub subscriber: Address,
    #[topic] pub merchant: Address,
    #[topic] pub amount: i128,
    pub billed_at: u64,
}

<<<<<<< feature/advanced-subscription-engine-bundle
=======
#[contractevent]
pub struct TrialStarted {
    #[topic] pub subscriber: Address,
    #[topic] pub merchant: Address,
    pub trial_duration: u64,
    pub started_at: u64,
}

#[contractevent]
pub struct TrialConverted {
    #[topic] pub subscriber: Address,
    #[topic] pub merchant: Address,
    pub converted_at: u64,
}

#[contractevent]
pub struct PaymentFailedGracePeriodStarted {
    #[topic] pub subscriber: Address,
    #[topic] pub merchant: Address,
    pub dunning_start_timestamp: u64,
    pub grace_period_end: u64,
}

#[contractevent]
pub struct SubscriptionUpgraded {
    #[topic] pub subscriber: Address,
    #[topic] pub merchant: Address,
    pub old_tier_id: u32,
    pub new_tier_id: u32,
    pub prorated_charge: i128,
    pub upgraded_at: u64,
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
>>>>>>> main

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
            let grace_period_end = sub.last_funds_exhausted.saturating_add(MAX_GRACE_PERIOD);
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

<<<<<<< feature/advanced-subscription-engine-bundle
        // Collect any pending earnings before changing rate
        distribute_and_collect(&env, &subscriber, &creator, Some(&creator));
        stream = get_stream(&env, &key);

        stream.tier.rate_per_second = new_rate_per_second;
        set_stream(&env, &key, &stream);

        TierChanged {
            subscriber: subscriber.clone(),
            creator: creator.clone(),
            old_rate,
            new_rate: new_rate_per_second,
        }.publish(&env);
    }

    /// Collect from all active streams for a creator in a single call.
    /// `max_count` caps the batch size to avoid hitting ledger instruction limits.
    /// Returns the total amount collected across all processed streams.
    pub fn withdraw_all(env: Env, creator: Address, max_count: u32) -> i128 {
        let subs_key = DataKey::CreatorSubscribers(creator.clone());
        let subs: Vec<Address> = env.storage().persistent().get(&subs_key).unwrap_or(vec![&env]);

        let mut total: i128 = 0;
        let limit = max_count.min(subs.len());
=======
        // Remove from blacklist
        env.storage().persistent().remove(&blacklist_key);
>>>>>>> main

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

    // --- Enhanced Subscription Functions for Issues #101-#104 ---
    
    // #101: Automated Pull-Payment Execution Module
    pub fn execute_subscription_pull(env: Env, merchant: Address, subscriber: Address) {
        merchant.require_auth();
        
        let billing_key = DataKey::BillingCycle(subscriber.clone(), merchant.clone());
        let billing_info: BillingCycleInfo = env.storage().persistent()
            .get(&billing_key)
            .expect("subscription not found");
            
        let now = env.ledger().timestamp();
        
        // Check if billing cycle has matured
        if now < billing_info.next_billing_date {
            panic!("billing premature");
        }
        
        // Check if subscription is in a state that allows billing
        match billing_info.status {
            SubscriptionStatus::Canceled => panic!("subscription canceled"),
            SubscriptionStatus::PastDue => {
                // Check if grace period has expired
                if now > billing_info.dunning_start_timestamp.saturating_add(MAX_GRACE_PERIOD) {
                    // Auto-cancel subscription
                    let mut updated_billing = billing_info.clone();
                    updated_billing.status = SubscriptionStatus::Canceled;
                    env.storage().persistent().set(&billing_key, &updated_billing);
                    panic!("grace period expired");
                }
            }
            _ => {} // Active and Trial can proceed
        }
        
        // Get subscription details
        let sub_key = subscription_key(&subscriber, &merchant);
        let mut subscription = get_subscription(&env, &sub_key);
        
        // Check if user has sufficient allowance
        let token_client = TokenClient::new(&env, &subscription.token);
        let allowance = token_client.allowance(&subscriber, &env.current_contract_address());
        
        if allowance < billing_info.billing_amount {
            // Payment failed - enter grace period if not already in it
            if billing_info.status == SubscriptionStatus::Active {
                let mut updated_billing = billing_info.clone();
                updated_billing.status = SubscriptionStatus::PastDue;
                updated_billing.dunning_start_timestamp = now;
                env.storage().persistent().set(&billing_key, &updated_billing);
                
                PaymentFailedGracePeriodStarted {
                    subscriber: subscriber.clone(),
                    merchant: merchant.clone(),
                    dunning_start_timestamp: now,
                    grace_period_end: now.saturating_add(MAX_GRACE_PERIOD),
                }.publish(&env);
            }
            panic!("insufficient allowance");
        }
        
        // Execute payment
        token_client.transfer(&subscriber, &merchant, &billing_info.billing_amount);
        
        // Update billing cycle
        let mut updated_billing = billing_info.clone();
        updated_billing.next_billing_date = now.saturating_add(billing_info.billing_cycle);
        
        // If we were in grace period, restore to active
        if billing_info.status == SubscriptionStatus::PastDue {
            updated_billing.status = SubscriptionStatus::Active;
            updated_billing.dunning_start_timestamp = 0;
        }
        
        env.storage().persistent().set(&billing_key, &updated_billing);
        
        // Update subscription balance and collected timestamp
        subscription.last_collected = now;
        subscription.balance += billing_info.billing_amount * PRECISION_MULTIPLIER;
        set_subscription(&env, &sub_key, &subscription);
        
        SubscriptionBilled {
            subscriber: subscriber.clone(),
            merchant: merchant.clone(),
            amount: billing_info.billing_amount,
            billed_at: now,
        }.publish(&env);
    }
<<<<<<< feature/advanced-subscription-engine-bundle

    /// #105: Cross-Asset Subscription Auto-Swaps
    /// Allows a user to pay in one asset and the merchant to receive another.
    pub fn execute_subscription_pull(
        env: Env,
        subscriber: Address,
        merchant: Address,
        asset_in: Address,
        amount_in_max: i128,
        asset_out: Address,
        amount_out: i128,
        dex_address: Address,
    ) {
        subscriber.require_auth();

        if asset_in == asset_out {
            let token_client = TokenClient::new(&env, &asset_in);
            token_client.transfer(&subscriber, &merchant, &amount_out);
        } else {
            let token_in = TokenClient::new(&env, &asset_in);
            
            // Atomic swap logic
            token_in.transfer(&subscriber, &env.current_contract_address(), &amount_in_max);
            
            // Mocking the cross-contract DEX call
            // In a real implementation, this would call a swap function on a DEX contract
            let amount_spent: i128 = env.invoke_contract(
                &dex_address,
                &soroban_sdk::symbol_short!("swap"),
                vec![
                    &env,
                    asset_in.clone().into_val(&env),
                    asset_out.clone().into_val(&env),
                    amount_in_max.into_val(&env),
                    amount_out.into_val(&env),
                ],
            );

            let token_out = TokenClient::new(&env, &asset_out);
            token_out.transfer(&env.current_contract_address(), &merchant, &amount_out);

            // Refund surplus to user
            let surplus = amount_in_max - amount_spent;
            if surplus > 0 {
                token_in.transfer(&env.current_contract_address(), &subscriber, &surplus);
            }

            CrossAssetBilled {
                subscriber,
                merchant,
                asset_in,
                asset_out,
                amount_in: amount_spent,
                amount_out,
            }.publish(&env);
        }
    }

    /// #106: "Pre-Authorization" Escrow Vaults for Annual Plans
    pub fn deposit_to_escrow(
        env: Env,
        subscriber: Address,
        merchant: Address,
        token: Address,
        amount: i128,
        duration_months: u32,
    ) {
        subscriber.require_auth();
        let token_client = TokenClient::new(&env, &token);
        token_client.transfer(&subscriber, &env.current_contract_address(), &amount);

        let now = env.ledger().timestamp();
        let duration_seconds = (duration_months as u64) * 30 * 24 * 60 * 60;
        
        let vault = EscrowVault {
            token,
            merchant: merchant.clone(),
            subscriber: subscriber.clone(),
            total_amount: amount,
            vested_amount: 0,
            start_time: now,
            end_time: now + duration_seconds,
            last_drip: now,
        };

        env.storage().persistent().set(&DataKey::Escrow(subscriber.clone(), merchant.clone()), &vault);

        AnnualEscrowLocked {
            subscriber: subscriber.clone(),
            merchant: merchant.clone(),
            amount,
            duration_months,
        }.publish(&env);
    }

    pub fn claim_drip(env: Env, subscriber: Address, merchant: Address) {
        let key = DataKey::Escrow(subscriber.clone(), merchant.clone());
        let mut vault: EscrowVault = env.storage().persistent().get(&key).expect("no escrow found");
        
        let now = env.ledger().timestamp();
        if now <= vault.last_drip {
            return;
        }

        let total_duration = (vault.end_time - vault.start_time) as i128;
        if total_duration == 0 { return; }

        let elapsed = (now.min(vault.end_time) - vault.last_drip) as i128;
        let drip_amount = (vault.total_amount * elapsed) / total_duration;

        if drip_amount > 0 {
            let token_client = TokenClient::new(&env, &vault.token);
            token_client.transfer(&env.current_contract_address(), &merchant, &drip_amount);
            vault.vested_amount += drip_amount;
            vault.last_drip = now.min(vault.end_time);
            env.storage().persistent().set(&key, &vault);
        }
    }

    pub fn refund_unvested(env: Env, subscriber: Address, merchant: Address) {
        subscriber.require_auth();
        let key = DataKey::Escrow(subscriber.clone(), merchant.clone());
        let vault: EscrowVault = env.storage().persistent().get(&key).expect("no escrow found");
        
        let unvested = vault.total_amount - vault.vested_amount;
        if unvested > 0 {
            let token_client = TokenClient::new(&env, &vault.token);
            token_client.transfer(&env.current_contract_address(), &subscriber, &unvested);
        }
        
        env.storage().persistent().remove(&key);
    }

    /// #108: Merchant Yield Routing for Idle Escrow Vaults
    pub fn set_yield_config(env: Env, merchant: Address, config: YieldConfig) {
        merchant.require_auth();
        if config.user_share_bps + config.merchant_share_bps + config.dao_share_bps != 10000 {
            panic!("shares must sum to 10000 bps");
=======
    
    // #102: Enhanced Trial Period and Auto-Conversion
    pub fn initialize_subscription(
        env: Env,
        subscriber: Address,
        merchant: Address,
        plan_id: u32,
        token: Address,
    ) {
        subscriber.require_auth();
        
        // Check if user has already used trial for this merchant
        let trial_used_key = DataKey::TrialUsed(subscriber.clone(), merchant.clone());
        if env.storage().persistent().has(&trial_used_key) {
            panic!("trial already used");
        }
        
        // Get plan details
        let plan_registry_key = DataKey::PlanRegistry(merchant.clone());
        let plans: soroban_sdk::Vec<Plan> = env.storage().persistent()
            .get(&plan_registry_key)
            .expect("no plans found");
            
        let plan = plans.iter()
            .find(|p| p.plan_id == plan_id && p.is_active)
            .expect("plan not found or inactive");
            
        let now = env.ledger().timestamp();
        
        // Create billing cycle info
        let billing_info = BillingCycleInfo {
            next_billing_date: if plan.has_trial {
                now.saturating_add(plan.trial_duration)
            } else {
                now
            },
            dunning_start_timestamp: 0,
            status: if plan.has_trial {
                SubscriptionStatus::Trial
            } else {
                SubscriptionStatus::Active
            },
            billing_amount: plan.billing_amount,
            billing_cycle: plan.billing_cycle,
        };
        
        // Store billing cycle info
        let billing_key = DataKey::BillingCycle(subscriber.clone(), merchant.clone());
        env.storage().persistent().set(&billing_key, &billing_info);
        
        // Mark trial as used if applicable
        if plan.has_trial {
            env.storage().persistent().set(&trial_used_key, &true);
            
            TrialStarted {
                subscriber: subscriber.clone(),
                merchant: merchant.clone(),
                trial_duration: plan.trial_duration,
                started_at: now,
            }.publish(&env);
>>>>>>> main
        }
        env.storage().persistent().set(&DataKey::YieldConfig(merchant), &config);
    }

    pub fn route_escrow_to_yield(env: Env, subscriber: Address, merchant: Address, amount: i128) {
        merchant.require_auth(); // Only merchant or relayer can route? Issue says protocol relayer.
        let key = DataKey::Escrow(subscriber.clone(), merchant.clone());
        let vault: EscrowVault = env.storage().persistent().get(&key).expect("no escrow found");
        
<<<<<<< feature/advanced-subscription-engine-bundle
        // Buffer: ensure we keep at least 30 days of drips in the contract
        let total_duration = (vault.end_time - vault.start_time) as i128;
        let monthly_buffer = (vault.total_amount * (30 * 24 * 60 * 60)) / total_duration;
        let current_unvested = vault.total_amount - vault.vested_amount;
        
        if amount > (current_unvested - monthly_buffer) {
            panic!("liquidity buffer violation");
        }

        let config: YieldConfig = env.storage().persistent().get(&DataKey::YieldConfig(merchant.clone())).expect("yield not configured");
        let token_client = TokenClient::new(&env, &vault.token);
        token_client.transfer(&env.current_contract_address(), &config.target_protocol, &amount);
    }

    pub fn harvest_yield(env: Env, merchant: Address, profit: i128) {
        let config: YieldConfig = env.storage().persistent().get(&DataKey::YieldConfig(merchant.clone())).expect("yield not configured");
        
        let user_amount = (profit * config.user_share_bps as i128) / 10000;
        let merchant_amount = (profit * config.merchant_share_bps as i128) / 10000;
        let dao_amount = profit - user_amount - merchant_amount;

        // In a real scenario, we'd pull from the protocol and distribute.
        // For now, we emit the event.
        YieldHarvested {
            merchant,
            profit,
            user_distributed: user_amount,
            merchant_distributed: merchant_amount,
            dao_distributed: dao_amount,
        }.publish(&env);
    }

    /// #107: ZK-Proof Anonymous Subscription Verification
    pub fn verify_anonymous_subscription(env: Env, merchant: Address, proof: Bytes, nullifier: Bytes) {
        if env.storage().persistent().has(&DataKey::Nullifier(nullifier.clone())) {
            panic!("replay attack detected");
        }

        // Mock ZK verification: proofs must be 64 bytes for this mock
        if proof.len() != 64 {
            panic!("invalid ZK proof");
        }

        env.storage().persistent().set(&DataKey::Nullifier(nullifier.clone()), &true);

        AccessGranted {
            merchant,
            nullifier,
        }.publish(&env);
=======
        // Create subscription using existing logic
        let tier = Tier {
            rate_per_second: plan.billing_amount / plan.billing_cycle as i128,
            trial_duration: if plan.has_trial { plan.trial_duration } else { 0 },
        };
        
        let subscription = Subscription {
            token: token.clone(),
            tier,
            balance: 0, // Start with zero balance for trial
            last_collected: now,
            start_time: now,
            last_funds_exhausted: 0,
            free_to_paid_emitted: false,
            creators: vec![&env, merchant.clone()],
            percentages: vec![&env, 100u32],
            payer: subscriber.clone(),
            beneficiary: subscriber.clone(),
            accrued_remainder: 0,
        };
        
        let sub_key = subscription_key(&subscriber, &merchant);
        set_subscription(&env, &sub_key, &subscription);
        
        Subscribed {
            subscriber: subscriber.clone(),
            creator: merchant.clone(),
            rate_per_second: plan.billing_amount / plan.billing_cycle as i128,
        }.publish(&env);
    }
    
    // #104: Tiered Subscription Upgrades and Proration Math
    pub fn upgrade_subscription_tier(
        env: Env,
        subscriber: Address,
        merchant: Address,
        new_tier_id: u32,
    ) {
        subscriber.require_auth();
        
        let billing_key = DataKey::BillingCycle(subscriber.clone(), merchant.clone());
        let mut billing_info: BillingCycleInfo = env.storage().persistent()
            .get(&billing_key)
            .expect("subscription not found");
            
        // Prevent downgrades
        if new_tier_id <= get_current_plan_id(&env, &merchant, billing_info.billing_amount) {
            panic!("cannot downgrade");
        }
        
        // Get new plan details
        let plan_registry_key = DataKey::PlanRegistry(merchant.clone());
        let plans: soroban_sdk::Vec<Plan> = env.storage().persistent()
            .get(&plan_registry_key)
            .expect("no plans found");
            
        let new_plan = plans.iter()
            .find(|p| p.plan_id == new_tier_id && p.is_active)
            .expect("new plan not found or inactive");
            
        let old_plan_id = get_current_plan_id(&env, &merchant, billing_info.billing_amount);
        
        // Calculate proration
        let now = env.ledger().timestamp();
        let cycle_elapsed = now.saturating_sub(billing_info.next_billing_date.saturating_sub(billing_info.billing_cycle));
        let cycle_remaining = billing_info.billing_cycle.saturating_sub(cycle_elapsed);
        
        // Calculate unused value: (remaining_time / total_time) * old_price
        let unused_value = (cycle_remaining as i128 * billing_info.billing_amount) / billing_info.billing_cycle as i128;
        
        // Calculate prorated difference
        let prorated_charge = new_plan.billing_amount.saturating_sub(unused_value);
        
        // Execute payment for prorated difference
        let sub_key = subscription_key(&subscriber, &merchant);
        let subscription = get_subscription(&env, &sub_key);
        let token_client = TokenClient::new(&env, &subscription.token);
        
        token_client.transfer(&subscriber, &merchant, &prorated_charge);
        
        // Update billing info
        billing_info.billing_amount = new_plan.billing_amount;
        billing_info.billing_cycle = new_plan.billing_cycle;
        billing_info.next_billing_date = now.saturating_add(new_plan.billing_cycle);
        env.storage().persistent().set(&billing_key, &billing_info);
        
        // Update subscription tier
        let mut updated_subscription = subscription;
        updated_subscription.tier.rate_per_second = new_plan.billing_amount / new_plan.billing_cycle as i128;
        updated_subscription.balance += prorated_charge * PRECISION_MULTIPLIER;
        set_subscription(&env, &sub_key, &updated_subscription);
        
        SubscriptionUpgraded {
            subscriber: subscriber.clone(),
            merchant: merchant.clone(),
            old_tier_id,
            new_tier_id,
            prorated_charge,
            upgraded_at: now,
        }.publish(&env);
    }
    
    // Helper function for merchants to register plans
    pub fn register_plan(env: Env, merchant: Address, plan: Plan) {
        merchant.require_auth();
        
        let plan_registry_key = DataKey::PlanRegistry(merchant.clone());
        let mut plans: soroban_sdk::Vec<Plan> = env.storage().persistent()
            .get(&plan_registry_key)
            .unwrap_or_else(|| vec![&env]);
            
        // Check if plan ID already exists
        for existing_plan in plans.iter() {
            if existing_plan.plan_id == plan.plan_id {
                panic!("plan ID already exists");
            }
        }
        
        plans.push_back(plan);
        env.storage().persistent().set(&plan_registry_key, &plans);
    }
    
    // Helper function to get subscription status
    pub fn get_subscription_status(env: Env, subscriber: Address, merchant: Address) -> SubscriptionStatus {
        let billing_key = DataKey::BillingCycle(subscriber, merchant);
        if let Some(billing_info) = env.storage().persistent().get::<BillingCycleInfo>(&billing_key) {
            billing_info.status
        } else {
            SubscriptionStatus::Canceled
        }
>>>>>>> main
    }

// --- Internal Logic & Helpers ---

// ... (rest of the code remains the same)
fn bump_instance_ttl(env: &Env) {
    // TTL bump functionality not available in this SDK version
}

fn subscription_key(subscriber: &Address, stream_id: &Address) -> DataKey {
    DataKey::Subscription(subscriber.clone(), stream_id.clone())
}

// ... (rest of the code remains the same)

fn set_subscription(env: &Env, key: &DataKey, sub: &Subscription) {
    if sub.balance > 0 {
        env.storage().persistent().set(key, sub);
        env.storage().temporary().remove(key);
        // Bump TTL for active subscriptions to keep them from expiring
        bump_instance_ttl(env);
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
        let grace_period_end = sub.last_funds_exhausted.saturating_add(MAX_GRACE_PERIOD);
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

// Helper function for plan ID lookup
fn get_current_plan_id(env: &Env, merchant: &Address, billing_amount: i128) -> u32 {
    let plan_registry_key = DataKey::PlanRegistry(merchant.clone());
    if let Some(plans) = env.storage().persistent().get::<soroban_sdk::Vec<Plan>>(&plan_registry_key) {
        for plan in plans.iter() {
            if plan.billing_amount == billing_amount && plan.is_active {
                return plan.plan_id;
            }
        }
    }
    0 // Default plan ID if not found
}

<<<<<<< feature/advanced-subscription-engine-bundle
// mod test;
mod test_issues;
=======
} // <--- Added this closing brace

#[cfg(test)]
mod test;
#[cfg(test)]
mod test_tiny_streams;
#[cfg(test)]
mod test_withdrawal_consistency;
#[cfg(test)]
mod test_enhanced_subscriptions;
>>>>>>> main
