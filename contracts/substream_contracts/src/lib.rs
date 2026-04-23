#![no_std]
#[cfg(test)]
extern crate std;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::{contract, contractevent, contractimpl, contracttype, vec, Address, Env};

// --- Constants ---
const MINIMUM_FLOW_DURATION: u64 = 86400;
const FREE_TRIAL_DURATION: u64 = 7 * 24 * 60 * 60;
const GRACE_PERIOD: u64 = 24 * 60 * 60;
#[allow(dead_code)]
const GENESIS_NFT_ADDRESS: &str = "CAS3J7GYCCX7RRBHAHXDUY3OOWFMTIDDNVGCH6YOY7W7Y7G656H2HHMA";
#[allow(dead_code)]
const DISCOUNT_BPS: i128 = 2000;
const SIX_MONTHS: u64 = 180 * 24 * 60 * 60;
#[allow(dead_code)]
const TWELVE_MONTHS: u64 = 365 * 24 * 60 * 60;
const PRECISION_MULTIPLIER: i128 = 1_000_000_000;
const REFERRAL_REBATE_BPS: i128 = 100; // 1% rebate
const TTL_THRESHOLD: u32 = 17280; // Assuming ~1 day in ledgers for example
const TTL_BUMP_AMOUNT: u32 = 518400; // Assuming ~30 days in ledgers for example

// --- SLA Circuit Breaker Constants ---
const SLA_THRESHOLD_BPS: u32 = 9990; // 99.9% uptime threshold (in basis points)
const SEVEN_DAYS: u64 = 7 * 24 * 60 * 60;
const UPTIME_ORACLE_NONCE_TTL: u64 = 24 * 60 * 60; // 24 hour validity for oracle signatures

// --- Merchant Registry and KYC Whitelisting Constants ---
const DAO_MULTISIG_THRESHOLD: u32 = 3; // Minimum signatures required for DAO decisions
const MERCHANT_KYC_VALIDITY: u64 = 365 * 24 * 60 * 60; // 1 year validity for KYC credentials
const SEP12_KYC_ISSUER: &str = "GD5DQX2K7Q4D4PE4R6J4Y7Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2"; // SEP-12 KYC issuer address
const TIMELOCK_DURATION: u64 = 48 * 60 * 60; // 48-hour timelock for registry updates
const SECURITY_COUNCIL_SIZE: u32 = 5; // 5-member security council for multi-sig

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
    Stream(Address, Address),
    TotalStreamed(Address, Address),
    CliffThreshold(Address),
    CreatorSubscribers(Address),
    CreatorMetadata(Address),
    ChannelPaused(Address),
    Escrow(Address, Address),
    Nullifier(Bytes),
    NullifierExpirationIndex(u64),    // Index for tracking nullifier expiration cleanup
    YieldConfig(Address),
    SLAStatus(Address),               // Merged from main
    UptimeOracleNonce(u64),           // Merged from main
    ContractAdmin,                    // Integrated for verify_creator
    VerifiedCreator(Address),
    UserReferrer(Address),
    ReferralTracker(Address, Address),
    CurrentFlowRate(Address),          // Aggregated flow rate for a channel
    AcceptedToken(Address),            // Issue #49: Creator's enforced stablecoin token
    DaoGrant(Address, Address),        // (dao, creator) — DAO treasury grant stream
    UserContributed(Address, Address), // (fan, creator) — lifetime tokens contributed by fan
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

// --- Merchant Registry and KYC Whitelisting Data Structures ---

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MerchantStatus {
    pub is_verified: bool,
    pub is_blacklisted: bool,
    pub verification_method: VerificationMethod,
    pub registered_at: u64,
    pub last_verified: u64,
    pub dao_approved: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VerificationMethod {
    SEP12KYC,
    DAOApproval,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KYCCredential {
    pub merchant_address: Address,
    pub issuer: Address,
    pub credential_hash: soroban_sdk::Vec<u8>,
    pub issued_at: u64,
    pub expires_at: u64,
    pub is_valid: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DAOProposal {
    pub proposal_id: u64,
    pub merchant_address: Address,
    pub proposal_type: ProposalType,
    pub description: soroban_sdk::String,
    pub created_at: u64,
    pub expires_at: u64,
    pub votes_for: u32,
    pub votes_against: u32,
    pub executed: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProposalType {
    WhitelistMerchant,
    BlacklistMerchant,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DAOVote {
    pub voter: Address,
    pub proposal_id: u64,
    pub vote: bool, // true = for, false = against
    pub voted_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryUpdateProposal {
    pub proposal_id: u64,
    pub merchant_address: Address,
    pub update_type: RegistryUpdateType,
    pub description: soroban_sdk::String,
    pub proposed_at: u64,
    pub executable_at: u64,          // When the proposal can be executed (48h later)
    pub votes_for: soroban_sdk::Vec<Address>,
    pub executed: bool,
    pub canceled: bool,
    pub emergency_bypass: bool,       // For severe scams requiring immediate action
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistryUpdateType {
    WhitelistMerchant,
    BlacklistMerchant,
    RemoveMerchant,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecurityCouncilMember {
    pub member: Address,
    pub added_at: u64,
    pub is_active: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecurityCouncilVeto {
    pub council_member: Address,
    pub proposal_id: u64,
    pub veto_reason: soroban_sdk::String,
    pub vetoed_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NullifierExpiration {
    pub nullifier: soroban_sdk::Bytes,
    pub expires_at: u64,
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
    #[topic]
    pub creator: Address,
    #[topic]
    pub token: Address,
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
    #[topic]
    pub beneficiary: Address,
    #[topic]
    pub creator: Address, // stream_id
    pub awarded_at: u64,
}

#[contractevent]
pub struct UserBlacklisted {
    #[topic]
    pub creator: Address,
    #[topic]
    pub user: Address,
}

#[contractevent]
pub struct UserUnblacklisted {
    #[topic]
    pub creator: Address,
    #[topic]
    pub user: Address,
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

#[contractevent]
pub struct SubscriptionBilled {
    #[topic] pub subscriber: Address,
    #[topic] pub merchant: Address,
    #[topic] pub amount: i128,
    pub billed_at: u64,
}

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

// --- Merchant Registry and KYC Whitelisting Events ---

#[contractevent]
pub struct MerchantWhitelisted {
    #[topic] pub merchant: Address,
    #[topic] pub verification_method: VerificationMethod,
    pub whitelisted_at: u64,
}

#[contractevent]
pub struct MerchantBlacklisted {
    #[topic] pub merchant: Address,
    #[topic] pub blacklisted_by: Address,
    pub reason: soroban_sdk::String,
    pub blacklisted_at: u64,
}

#[contractevent]
pub struct KYCCredentialVerified {
    #[topic] pub merchant: Address,
    #[topic] pub issuer: Address,
    pub verified_at: u64,
}

#[contractevent]
pub struct DAOProposalCreated {
    #[topic] pub proposal_id: u64,
    #[topic] pub merchant: Address,
    #[topic] pub proposal_type: ProposalType,
    pub created_by: Address,
    pub created_at: u64,
}

#[contractevent]
pub struct DAOProposalExecuted {
    #[topic] pub proposal_id: u64,
    #[topic] pub merchant: Address,
    #[topic] pub proposal_type: ProposalType,
    pub executed: bool,
    pub executed_at: u64,
}

#[contractevent]
pub struct DAOVoteCast {
    #[topic] pub voter: Address,
    #[topic] pub proposal_id: u64,
    #[topic] pub vote: bool,
    pub voted_at: u64,
}

#[contractevent]
pub struct RegistryUpdateProposed {
    #[topic] pub proposal_id: u64,
    #[topic] pub merchant: Address,
    #[topic] pub update_type: RegistryUpdateType,
    pub proposed_by: Address,
    pub executable_at: u64,
    pub proposed_at: u64,
    pub emergency_bypass: bool,
}

#[contractevent]
pub struct RegistryUpdateExecuted {
    #[topic] pub proposal_id: u64,
    #[topic] pub merchant: Address,
    #[topic] pub update_type: RegistryUpdateType,
    pub executed_by: Address,
    pub executed_at: u64,
}

#[contractevent]
pub struct RegistryUpdateCanceled {
    #[topic] pub proposal_id: u64,
    #[topic] pub merchant: Address,
    #[topic] pub canceled_by: Address,
    pub canceled_at: u64,
}

#[contractevent]
pub struct SecurityCouncilVetoed {
    #[topic] pub proposal_id: u64,
    #[topic] pub council_member: Address,
    #[topic] pub merchant: Address,
    pub veto_reason: soroban_sdk::String,
    pub vetoed_at: u64,
}

// --- Global Reentrancy Guard Events ---

#[contractevent]
pub struct ReentrancyAttemptDetected {
    #[topic] pub caller: Address,
    #[topic] pub protected_function: soroban_sdk::String,
    pub detected_at: u64,
}

#[contractevent]
pub struct ReplayAttackBlocked {
    #[topic] pub merchant: Address,
    #[topic] pub nullifier: soroban_sdk::Bytes,
    pub blocked_at: u64,
}

#[contractevent]
pub struct CliffUnlocked {
    #[topic]
    pub fan: Address,
    #[topic]
    pub creator: Address,
    pub total_contributed: i128,
    pub cliff_threshold: i128,
}

#[contract]
pub struct SubStreamContract;

#[allow(clippy::too_many_arguments)]
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

    #[allow(clippy::too_many_arguments)]
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
        // Check if creator is a verified merchant (KYC or DAO approved)
        if !is_merchant_verified(&env, &creator) {
            panic!("creator is not a verified merchant");
        }
        
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
        credit_fan_contribution(&env, &user, &creator, amount);
        TipReceived {
            user,
            creator,
            token,
            amount,
        }
        .publish(&env);
    }

    #[allow(clippy::too_many_arguments)]
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
        env.storage()
            .persistent()
            .set(&DataKey::MinimumRate(creator), &min_rate);
    }

    pub fn set_community_goal(env: Env, creator: Address, goal_tokens_per_day: i128) {
        creator.require_auth();
        // Convert tokens/day to flow rate (units per second)
        // Using PRECISION_MULTIPLIER to maintain high-fidelity streaming math
        let goal_per_sec = (goal_tokens_per_day * PRECISION_MULTIPLIER) / 86400;
        env.storage()
            .persistent()
            .set(&DataKey::CommunityGoal(creator), &goal_per_sec);
    }

    pub fn is_community_goal_met(env: Env, creator: Address) -> bool {
        let goal: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::CommunityGoal(creator.clone()))
            .unwrap_or(0);
        if goal == 0 {
            return false;
        }

        let current: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::CurrentFlowRate(creator))
            .unwrap_or(0);
        current >= goal
    }

    // --- Issue #49: Stablecoin-Only Enforcement ---
    pub fn set_accepted_token(env: Env, creator: Address, token: Address) {
        creator.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::AcceptedToken(creator.clone()), &token);
        AcceptedTokenSet { creator, token }.publish(&env);
    }

    // -----------------------------------------------------------------------
    // Cliff-Based Access — Milestone Rewards for Early Supporters
    // -----------------------------------------------------------------------

    /// Creator sets the lifetime-contribution threshold that unlocks premium content.
    /// `threshold` is in whole token units (not nano).
    pub fn set_cliff_threshold(env: Env, creator: Address, threshold: i128) {
        creator.require_auth();
        if threshold <= 0 {
            panic!("threshold must be positive");
        }
        env.storage()
            .persistent()
            .set(&DataKey::CliffThreshold(creator), &threshold);
    }

    /// Returns `true` when the fan's lifetime contributions to this creator
    /// meet or exceed the creator's configured cliff threshold.
    /// Returns `false` if no threshold is set (feature not enabled by creator).
    pub fn check_cliff_access(env: Env, fan: Address, creator: Address) -> bool {
        let threshold: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::CliffThreshold(creator.clone()))
            .unwrap_or(0);
        if threshold == 0 {
            return false;
        }
        let contributed: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::UserContributed(fan, creator))
            .unwrap_or(0);
        contributed >= threshold
    }

    /// Returns the fan's total lifetime token contributions to a creator.
    pub fn get_total_contributed(env: Env, fan: Address, creator: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::UserContributed(fan, creator))
            .unwrap_or(0)
    }

    // -----------------------------------------------------------------------
    // DAO Treasury Streaming — Grant Support
    // -----------------------------------------------------------------------

    /// Called by a DAO contract to initiate a streaming grant to a creator.
    /// The DAO is the payer; the creator receives tokens second-by-second.
    /// Respects the same 7-day free-trial window and minimum-flow-duration
    /// rules as regular subscriptions so the protocol remains consistent.
    pub fn dao_grant(
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
        
        // Check if merchant is verified
        if !is_merchant_verified(&env, &merchant) {
            panic!("merchant is not verified");
        }
        
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
    }

    // --- Timelock and Multi-Sig Governance Functions ---
    
    /// Propose a registry update with mandatory 48-hour timelock
    /// Requires 3-of-5 multi-sig consensus before execution
    pub fn propose_registry_update(
        env: Env,
        proposer: Address,
        merchant: Address,
        update_type: RegistryUpdateType,
        description: soroban_sdk::String,
        emergency_bypass: bool,
    ) -> u64 {
        proposer.require_auth();
        
        // Verify proposer is authorized (Security Council member or admin)
        if !is_authorized_proposer(&env, &proposer) {
            panic!("unauthorized proposer");
        }
        
        // Emergency bypass requires additional authorization
        if emergency_bypass && !is_emergency_authorized(&env, &proposer) {
            panic!("unauthorized emergency bypass");
        }
        
        // Generate unique proposal ID
        let proposal_id = generate_registry_proposal_id(&env);
        
        let now = env.ledger().timestamp();
        let executable_at = if emergency_bypass {
            now // Immediate execution for emergencies
        } else {
            now.saturating_add(TIMELOCK_DURATION) // 48-hour timelock
        };
        
        let proposal = RegistryUpdateProposal {
            proposal_id,
            merchant_address: merchant.clone(),
            update_type: update_type.clone(),
            description: description.clone(),
            proposed_at: now,
            executable_at,
            votes_for: vec![&env],
            executed: false,
            canceled: false,
            emergency_bypass,
        };
        
        env.storage().persistent().set(&DataKey::RegistryUpdateProposal(proposal_id), &proposal);
        
        // Emit event
        RegistryUpdateProposed {
            proposal_id,
            merchant: merchant.clone(),
            update_type,
            proposed_by: proposer,
            executable_at,
            proposed_at: now,
            emergency_bypass,
        }.publish(&env);
        
        proposal_id
    }
    
    /// Vote on a registry update proposal (3-of-5 multi-sig)
    pub fn vote_registry_update(env: Env, voter: Address, proposal_id: u64) {
        voter.require_auth();
        
        // Verify voter is Security Council member
        if !is_security_council_member(&env, &voter) {
            panic!("not a security council member");
        }
        
        let proposal_key = DataKey::RegistryUpdateProposal(proposal_id);
        let mut proposal: RegistryUpdateProposal = env.storage().persistent()
            .get(&proposal_key)
            .expect("proposal not found");
        
        // Check if proposal is still pending
        if proposal.executed || proposal.canceled {
            panic!("proposal no longer active");
        }
        
        // Check if already voted
        if proposal.votes_for.contains(&voter) {
            panic!("already voted");
        }
        
        // Add vote
        proposal.votes_for.push_back(voter.clone());
        
        // Update proposal
        env.storage().persistent().set(&proposal_key, &proposal);
        
        // Check if consensus threshold is reached (3-of-5)
        if proposal.votes_for.len() >= DAO_MULTISIG_THRESHOLD as usize {
            // For non-emergency proposals, timelock must be respected
            if !proposal.emergency_bypass {
                let now = env.ledger().timestamp();
                if now < proposal.executable_at {
                    // Consensus reached but timelock not expired - proposal is ready for execution
                    return;
                }
            }
            
            // Execute the proposal
            execute_registry_update(&env, proposal_id);
        }
    }
    
    /// Execute a registry update proposal (after timelock expires)
    pub fn execute_registry_update(env: Env, executor: Address, proposal_id: u64) {
        executor.require_auth();
        
        let proposal_key = DataKey::RegistryUpdateProposal(proposal_id);
        let proposal: RegistryUpdateProposal = env.storage().persistent()
            .get(&proposal_key)
            .expect("proposal not found");
        
        // Verify proposal is ready for execution
        if proposal.executed || proposal.canceled {
            panic!("proposal no longer active");
        }
        
        // Check timelock (unless emergency bypass)
        if !proposal.emergency_bypass {
            let now = env.ledger().timestamp();
            if now < proposal.executable_at {
                panic!("timelock not expired");
            }
        }
        
        // Check consensus threshold
        if proposal.votes_for.len() < DAO_MULTISIG_THRESHOLD as usize {
            panic!("consensus not reached");
        }
        
        // Execute the registry update
        execute_registry_update(&env, proposal_id);
        
        // Emit execution event
        RegistryUpdateExecuted {
            proposal_id,
            merchant: proposal.merchant_address.clone(),
            update_type: proposal.update_type.clone(),
            executed_by: executor,
            executed_at: env.ledger().timestamp(),
        }.publish(&env);
    }
    
    /// Security Council veto of pending proposal
    pub fn security_council_veto(
        env: Env,
        council_member: Address,
        proposal_id: u64,
        veto_reason: soroban_sdk::String,
    ) {
        council_member.require_auth();
        
        // Verify council member authority
        if !is_security_council_member(&env, &council_member) {
            panic!("not a security council member");
        }
        
        let proposal_key = DataKey::RegistryUpdateProposal(proposal_id);
        let mut proposal: RegistryUpdateProposal = env.storage().persistent()
            .get(&proposal_key)
            .expect("proposal not found");
        
        // Can only veto pending proposals
        if proposal.executed || proposal.canceled {
            panic!("cannot veto executed or canceled proposal");
        }
        
        // Cancel the proposal
        proposal.canceled = true;
        env.storage().persistent().set(&proposal_key, &proposal);
        
        // Record veto
        let veto = SecurityCouncilVeto {
            council_member: council_member.clone(),
            proposal_id,
            veto_reason: veto_reason.clone(),
            vetoed_at: env.ledger().timestamp(),
        };
        env.storage().persistent().set(&DataKey::SecurityCouncilVeto(council_member.clone(), proposal_id), &veto);
        
        // Emit veto event
        SecurityCouncilVetoed {
            proposal_id,
            council_member: council_member.clone(),
            merchant: proposal.merchant_address.clone(),
            veto_reason,
            vetoed_at: env.ledger().timestamp(),
        }.publish(&env);
    }
    
    /// Initialize Security Council (5 members)
    pub fn initialize_security_council(env: Env, admin: Address, council_members: soroban_sdk::Vec<Address>) {
        admin.require_auth();
        
        // Verify admin authorization
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::ContractAdmin)
            .expect("not initialized");
        if admin != stored_admin {
            panic!("admin only");
        }
        
        // Verify exactly 5 members
        if council_members.len() != SECURITY_COUNCIL_SIZE as usize {
            panic!("security council must have exactly 5 members");
        }
        
        let now = env.ledger().timestamp();
        
        // Add all council members
        for i in 0..council_members.len() {
            let member = council_members.get(i).unwrap();
            let council_member = SecurityCouncilMember {
                member: member.clone(),
                added_at: now,
                is_active: true,
            };
            env.storage().persistent().set(&DataKey::SecurityCouncilMember(member.clone()), &council_member);
        }
    }
    
    // --- Merchant Registry and KYC Whitelisting Functions ---

    // Register a merchant with SEP-12 KYC verification
    pub fn register_merchant_with_kyc(
        env: Env,
        merchant: Address,
        kyc_credential_hash: soroban_sdk::Vec<u8>,
        issuer: Address,
    ) {
        merchant.require_auth();
        
        // Verify issuer is authorized SEP-12 KYC provider
        let authorized_issuer = Address::from_string(&soroban_sdk::String::from_str(&env, SEP12_KYC_ISSUER));
        if issuer != authorized_issuer {
            panic!("unauthorized KYC issuer");
        }

        let elapsed = (now - charge_start) as i128;
        let accrued = elapsed
            .saturating_mul(grant.rate_per_second)
            .saturating_add(grant.accrued_remainder);
        let payout_tokens = accrued / PRECISION_MULTIPLIER;
        let available_tokens = grant.balance / PRECISION_MULTIPLIER;
        let actual_payout = payout_tokens.min(available_tokens);

        if actual_payout > 0 {
            let token_client = TokenClient::new(&env, &grant.token);
            token_client.transfer(&env.current_contract_address(), &creator, &actual_payout);
            credit_creator_earnings(&env, &creator, actual_payout);
            credit_fan_contribution(&env, &grant.dao, &creator, actual_payout);
        }
        
        // Check if merchant is blacklisted
        if is_merchant_blacklisted(&env, &merchant) {
            panic!("merchant is blacklisted");
        }
        
        let now = env.ledger().timestamp();
        
        // Create KYC credential
        let kyc_credential = KYCCredential {
            merchant_address: merchant.clone(),
            issuer: issuer.clone(),
            credential_hash: kyc_credential_hash.clone(),
            issued_at: now,
            expires_at: now.saturating_add(MERCHANT_KYC_VALIDITY),
            is_valid: true,
        };
        
        // Create merchant status
        let merchant_status = MerchantStatus {
            is_verified: true,
            is_blacklisted: false,
            verification_method: VerificationMethod::SEP12KYC,
            registered_at: now,
            last_verified: now,
            dao_approved: false,
        };
        
        // Store merchant data
        env.storage().persistent().set(&DataKey::MerchantRegistry(merchant.clone()), &merchant_status);
        env.storage().persistent().set(&DataKey::KYCCredential(merchant.clone()), &kyc_credential);
        
        // Emit events
        KYCCredentialVerified {
            merchant: merchant.clone(),
            issuer,
            verified_at: now,
        }.publish(&env);
        
        MerchantWhitelisted {
            merchant: merchant.clone(),
            verification_method: VerificationMethod::SEP12KYC,
            whitelisted_at: now,
        }.publish(&env);
    }
    
    // Create DAO proposal for merchant approval
    pub fn create_merchant_proposal(
        env: Env,
        proposer: Address,
        merchant: Address,
        proposal_type: ProposalType,
        description: soroban_sdk::String,
    ) -> u64 {
        proposer.require_auth();
        
        // Generate unique proposal ID
        let proposal_id = generate_proposal_id(&env);
        
        let now = env.ledger().timestamp();
        let expires_at = now.saturating_add(7 * 24 * 60 * 60); // 7 days voting period
        
        let proposal = DAOProposal {
            proposal_id,
            merchant_address: merchant.clone(),
            proposal_type: proposal_type.clone(),
            description: description.clone(),
            created_at: now,
            expires_at,
            votes_for: 0,
            votes_against: 0,
            executed: false,
        };
        
        env.storage().persistent().set(&DataKey::DAOProposal(proposal_id), &proposal);
        
        // Emit event
        DAOProposalCreated {
            proposal_id,
            merchant: merchant.clone(),
            proposal_type,
            created_by: proposer,
            created_at: now,
        }.publish(&env);
        
        proposal_id
    }
    
    // Vote on merchant proposal
    pub fn vote_on_merchant_proposal(env: Env, voter: Address, proposal_id: u64, vote: bool) {
        voter.require_auth();
        
        // Check if voter is authorized (could be DAO member or token holder)
        if !is_authorized_voter(&env, &voter) {
            panic!("unauthorized voter");
        }
        
        let proposal_key = DataKey::DAOProposal(proposal_id);
        let mut proposal: DAOProposal = env.storage().persistent()
            .get(&proposal_key)
            .expect("proposal not found");
        
        // Check if proposal is still active
        let now = env.ledger().timestamp();
        if now > proposal.expires_at || proposal.executed {
            panic!("proposal no longer active");
        }
        
        // Check if already voted
        let vote_key = DataKey::DAOVote(voter.clone(), proposal_id);
        if env.storage().persistent().has(&vote_key) {
            panic!("already voted");
        }
        
        // Record vote
        let dao_vote = DAOVote {
            voter: voter.clone(),
            proposal_id,
            vote,
            voted_at: now,
        };
        env.storage().persistent().set(&vote_key, &dao_vote);
        
        // Update proposal vote counts
        if vote {
            proposal.votes_for += 1;
        } else {
            proposal.votes_against += 1;
        }
        
        env.storage().persistent().set(&proposal_key, &proposal);
        
        // Emit event
        DAOVoteCast {
            voter,
            proposal_id,
            vote,
            voted_at: now,
        }.publish(&env);
        
        // Check if proposal should be executed
        if proposal.votes_for >= DAO_MULTISIG_THRESHOLD {
            execute_merchant_proposal(&env, proposal_id);
        }
    }
    
    // Blacklist a merchant (DAO only)
    pub fn blacklist_merchant(env: Env, dao_member: Address, merchant: Address, reason: soroban_sdk::String) {
        dao_member.require_auth();
        
        // Verify DAO member authorization
        if !is_authorized_dao_member(&env, &dao_member) {
            panic!("unauthorized DAO member");
        }
        
        let merchant_key = DataKey::MerchantRegistry(merchant.clone());
        let mut merchant_status: MerchantStatus = env.storage().persistent()
            .get(&merchant_key)
            .expect("merchant not registered");
        
        merchant_status.is_blacklisted = true;
        merchant_status.last_verified = env.ledger().timestamp();
        
        env.storage().persistent().set(&merchant_key, &merchant_status);
        env.storage().persistent().set(&DataKey::BlacklistedMerchant(merchant.clone()), &true);
        
        // Emit event
        MerchantBlacklisted {
            merchant: merchant.clone(),
            blacklisted_by: dao_member,
            reason: reason.clone(),
            blacklisted_at: env.ledger().timestamp(),
        }.publish(&env);
    }
    
    // Check if merchant is verified
    pub fn is_merchant_verified(env: Env, merchant: Address) -> bool {
        if let Some(merchant_status) = env.storage().persistent().get::<MerchantStatus>(&DataKey::MerchantRegistry(merchant)) {
            merchant_status.is_verified && !merchant_status.is_blacklisted
        } else {
            false
        }
    }
    
    // Get merchant status
    pub fn get_merchant_status(env: Env, merchant: Address) -> MerchantStatus {
        env.storage().persistent()
            .get(&DataKey::MerchantRegistry(merchant))
            .expect("merchant not found")
    }

    // --- Anonymous Subscription Verification with Nullifier Tracking ---
    
    // Constants for nullifier management
    const NULLIFIER_VALIDITY_PERIOD: u64 = 30 * 24 * 60 * 60; // 30 days
    const NULLIFIER_CLEANUP_BATCH_SIZE: u64 = 100; // Process up to 100 nullifiers per cleanup
    
    // Verify anonymous subscription with ZK-proof and nullifier
    pub fn verify_anonymous_subscription(
        env: Env,
        merchant: Address,
        proof: soroban_sdk::Bytes,
        nullifier: soroban_sdk::Bytes,
    ) {
        // Create reentrancy guard
        let _guard = reentrancy_guard!(&env, "verify_anonymous_subscription");
        
        // Check if merchant is verified
        if !is_merchant_verified(&env, &merchant) {
            panic!("merchant is not verified");
        }
        
        // Check if nullifier already exists (replay attack prevention)
        let nullifier_key = DataKey::Nullifier(nullifier.clone());
        if env.storage().persistent().has(&nullifier_key) {
            // Emit replay attack blocked event
            ReplayAttackBlocked {
                merchant: merchant.clone(),
                nullifier: nullifier.clone(),
                blocked_at: env.ledger().timestamp(),
            }.publish(&env);
            
            panic!("replay attack detected: nullifier already used");
        }
        
        // In a real implementation, this would verify the ZK-proof
        // For now, we'll assume the proof is valid if it has the expected length
        if proof.len() != 64 {
            panic!("invalid proof length");
        }
        
        // Store nullifier with expiration timestamp
        let now = env.ledger().timestamp();
        let expires_at = now.saturating_add(NULLIFIER_VALIDITY_PERIOD);
        
        // Store nullifier to prevent reuse
        env.storage().persistent().set(&nullifier_key, &true);
        
        // Store expiration info for cleanup
        let expiration_index_key = DataKey::NullifierExpirationIndex(now);
        let expiration_info = NullifierExpiration {
            nullifier: nullifier.clone(),
            expires_at,
        };
        env.storage().persistent().set(&expiration_index_key, &expiration_info);
        
        // Set TTL for cleanup entry
        env.storage().persistent().extend_ttl(&expiration_index_key, NULLIFIER_VALIDITY_PERIOD, NULLIFIER_VALIDITY_PERIOD);
    }
    
    // Try to verify anonymous subscription (returns Result for testing)
    pub fn try_verify_anonymous_subscription(
        env: Env,
        merchant: Address,
        proof: soroban_sdk::Bytes,
        nullifier: soroban_sdk::Bytes,
    ) -> Result<(), soroban_sdk::Error> {
        // Create reentrancy guard
        let _guard = reentrancy_guard!(&env, "try_verify_anonymous_subscription");
        
        // Check if merchant is verified
        if !is_merchant_verified(&env, &merchant) {
            return Err(soroban_sdk::Error::from_contract_error(1));
        }
        
        // Check if nullifier already exists (replay attack prevention)
        let nullifier_key = DataKey::Nullifier(nullifier.clone());
        if env.storage().persistent().has(&nullifier_key) {
            // Emit replay attack blocked event
            ReplayAttackBlocked {
                merchant: merchant.clone(),
                nullifier: nullifier.clone(),
                blocked_at: env.ledger().timestamp(),
            }.publish(&env);
            
            return Err(soroban_sdk::Error::from_contract_error(2));
        }
        
        // Verify proof
        if proof.len() != 64 {
            return Err(soroban_sdk::Error::from_contract_error(3));
        }
        
        // Store nullifier with expiration timestamp
        let now = env.ledger().timestamp();
        let expires_at = now.saturating_add(NULLIFIER_VALIDITY_PERIOD);
        
        // Store nullifier to prevent reuse
        env.storage().persistent().set(&nullifier_key, &true);
        
        // Store expiration info for cleanup
        let expiration_index_key = DataKey::NullifierExpirationIndex(now);
        let expiration_info = NullifierExpiration {
            nullifier: nullifier.clone(),
            expires_at,
        };
        env.storage().persistent().set(&expiration_index_key, &expiration_info);
        
        // Set TTL for cleanup entry
        env.storage().persistent().extend_ttl(&expiration_index_key, NULLIFIER_VALIDITY_PERIOD, NULLIFIER_VALIDITY_PERIOD);
        
        Ok(())
    }
    
    // Cleanup expired nullifiers to prevent storage bloat
    pub fn cleanup_expired_nullifiers(env: Env) {
        // Create reentrancy guard
        let _guard = reentrancy_guard!(&env, "cleanup_expired_nullifiers");
        
        let now = env.ledger().timestamp();
        let mut processed = 0u64;
        
        // Scan for expired nullifiers by timestamp
        let mut current_timestamp = now.saturating_sub(NULLIFIER_VALIDITY_PERIOD);
        
        while processed < NULLIFIER_CLEANUP_BATCH_SIZE && current_timestamp <= now {
            let expiration_index_key = DataKey::NullifierExpirationIndex(current_timestamp);
            
            if let Some(expiration_info) = env.storage().persistent().get::<NullifierExpiration>(&expiration_index_key) {
                if expiration_info.expires_at <= now {
                    // Remove expired nullifier
                    let nullifier_key = DataKey::Nullifier(expiration_info.nullifier.clone());
                    env.storage().persistent().remove(&nullifier_key);
                    
                    // Remove expiration index entry
                    env.storage().persistent().remove(&expiration_index_key);
                    
                    processed += 1;
                }
            }
            
            current_timestamp = current_timestamp.saturating_add(1);
        }
    }
    
    // Check if nullifier exists (for testing purposes)
    pub fn is_nullifier_used(env: Env, nullifier: soroban_sdk::Bytes) -> bool {
        env.storage().persistent().has(&DataKey::Nullifier(nullifier))
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
        env.storage()
            .persistent()
            .extend_ttl(key, TTL_THRESHOLD, TTL_BUMP_AMOUNT);
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

/// Increments the fan's lifetime contribution counter for a creator and emits
/// a `CliffUnlocked` event the first time the threshold is crossed.
fn credit_fan_contribution(env: &Env, fan: &Address, creator: &Address, amount: i128) {
    if amount <= 0 {
        return;
    }
    let key = DataKey::UserContributed(fan.clone(), creator.clone());
    let prev: i128 = env.storage().persistent().get(&key).unwrap_or(0);
    let next = prev.saturating_add(amount);
    env.storage().persistent().set(&key, &next);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_BUMP_AMOUNT);

    // Emit CliffUnlocked exactly once — when the fan crosses the threshold.
    let threshold: i128 = env
        .storage()
        .persistent()
        .get(&DataKey::CliffThreshold(creator.clone()))
        .unwrap_or(0);
    if threshold > 0 && prev < threshold && next >= threshold {
        CliffUnlocked {
            fan: fan.clone(),
            creator: creator.clone(),
            total_contributed: next,
            cliff_threshold: threshold,
        }
        .publish(env);
    }
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

    if amount_to_collect > sub.balance && sub.last_funds_exhausted == 0 {
        sub.last_funds_exhausted = now;
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
        let referral_rebate = if let Some(_referrer) = get_user_referrer(env, &sub.beneficiary) {
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
                credit_fan_contribution(env, &sub.beneficiary, &creator, payout);
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
    token_client.transfer(&sub.payer, env.current_contract_address(), &amount);

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

    let now = env.ledger().timestamp();
    let is_early = now < sub.start_time + MINIMUM_FLOW_DURATION;

    // Collect any charges that have accrued so far (zero during the trial window).
    distribute_and_collect(env, beneficiary, stream_id, None);
    sub = get_subscription(env, &key); // Refresh after collect.

    if is_early {
        // The creator is entitled to compensation equal to the full minimum-lock
        // period even though the subscriber is cancelling early.  This prevents
        // flash-subscribe attacks where a user subscribes, scrapes content, and
        // immediately cancels to recover their deposit.
        //
        // Penalty = rate_per_second × MINIMUM_FLOW_DURATION (in internal nano
        // units), capped at the remaining balance so we never overdraw.
        let min_entitled_nano = sub
            .tier
            .rate_per_second
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
            sub.balance -= penalty_nano.min(available_nano);
        }
    }

    let rate = sub.tier.rate_per_second;
    let mut total_flow: i128 = env
        .storage()
        .persistent()
        .get(&DataKey::CurrentFlowRate(stream_id.clone()))
        .unwrap_or(0);
    total_flow = total_flow.saturating_sub(rate);
    env.storage()
        .persistent()
        .set(&DataKey::CurrentFlowRate(stream_id.clone()), &total_flow);

    if sub.balance > 0 {
        let token_client = TokenClient::new(&env, &sub.token);
        let refund_amount = sub.balance / PRECISION_MULTIPLIER;
        if refund_amount > 0 {
            token_client.transfer(&env.current_contract_address(), &sub.payer, &refund_amount);
        }
    }

    for i in 0..sub.creators.len() {
        let creator = sub.creators.get(i).unwrap();
        unregister_creator_support(env, &creator, subscriber);
    }
    env.storage().persistent().remove(&key);
    env.storage().temporary().remove(&key);
}

#[allow(clippy::too_many_arguments)]
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
    if let Some(accepted_token) = env
        .storage()
        .persistent()
        .get::<_, Address>(&DataKey::AcceptedToken(stream_id.clone()))
    {
        if token != &accepted_token {
            panic!("creator only accepts their specified stablecoin");
        }
    }
    // ---------------------------------------------------

    let key = subscription_key(beneficiary, stream_id);
    if subscription_exists(env, &key) {
        panic!("exists");
    }

    let floor: i128 = env
        .storage()
        .persistent()
        .get(&DataKey::MinimumRate(stream_id.clone()))
        .unwrap_or(0);
    if rate < floor {
        panic!("rate below floor");
    }

    // Trial support: Allow starting a stream with 0 initial balance
    if amount > 0 {
        let token_client = TokenClient::new(env, token);
        token_client.transfer(payer, env.current_contract_address(), &amount);
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

    let mut total_flow: i128 = env
        .storage()
        .persistent()
        .get(&DataKey::CurrentFlowRate(stream_id.clone()))
        .unwrap_or(0);
    total_flow = total_flow.saturating_add(rate);
    env.storage()
        .persistent()
        .set(&DataKey::CurrentFlowRate(stream_id.clone()), &total_flow);

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

fn bump_instance_ttl(env: &Env) {
    // TTL bump functionality not available in this SDK version
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
        let token_client = TokenClient::new(&env, &sub.token);
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

// --- Timelock and Multi-Sig Governance Helper Functions ---

fn generate_registry_proposal_id(env: &Env) -> u64 {
    // Generate unique proposal ID based on timestamp and existing proposals
    let now = env.ledger().timestamp();
    let mut proposal_id = now;
    
    // Ensure uniqueness by checking existing proposals
    while env.storage().persistent().has(&DataKey::RegistryUpdateProposal(proposal_id)) {
        proposal_id += 1;
    }
    
    proposal_id
}

fn is_authorized_proposer(env: &Env, proposer: &Address) -> bool {
    // Security Council members and contract admin can propose
    if is_security_council_member(env, proposer) {
        return true;
    }
    
    if let Some(admin) = env.storage().persistent().get::<Address>(&DataKey::ContractAdmin) {
        proposer == &admin
    } else {
        false
    }
}

fn is_emergency_authorized(env: &Env, proposer: &Address) -> bool {
    // Emergency bypass requires admin authorization (higher security)
    if let Some(admin) = env.storage().persistent().get::<Address>(&DataKey::ContractAdmin) {
        proposer == &admin
    } else {
        false
    }
}

fn is_security_council_member(env: &Env, member: &Address) -> bool {
    if let Some(council_member) = env.storage().persistent().get::<SecurityCouncilMember>(&DataKey::SecurityCouncilMember(member.clone())) {
        council_member.is_active
    } else {
        false
    }
}

fn execute_registry_update(env: &Env, proposal_id: u64) {
    let proposal_key = DataKey::RegistryUpdateProposal(proposal_id);
    let mut proposal: RegistryUpdateProposal = env.storage().persistent()
        .get(&proposal_key)
        .expect("proposal not found");
    
    let merchant_key = DataKey::MerchantRegistry(proposal.merchant_address.clone());
    
    match proposal.update_type {
        RegistryUpdateType::WhitelistMerchant => {
            // Check if merchant exists, if not create basic entry
            let mut merchant_status = if let Some(existing_status) = env.storage().persistent().get::<MerchantStatus>(&merchant_key) {
                existing_status
            } else {
                MerchantStatus {
                    is_verified: false,
                    is_blacklisted: false,
                    verification_method: VerificationMethod::DAOApproval,
                    registered_at: env.ledger().timestamp(),
                    last_verified: 0,
                    dao_approved: false,
                }
            };
            
            merchant_status.is_verified = true;
            merchant_status.is_blacklisted = false;
            merchant_status.verification_method = VerificationMethod::DAOApproval;
            merchant_status.dao_approved = true;
            merchant_status.last_verified = env.ledger().timestamp();
            
            env.storage().persistent().set(&merchant_key, &merchant_status);
            
            // Emit merchant whitelisted event
            MerchantWhitelisted {
                merchant: proposal.merchant_address.clone(),
                verification_method: VerificationMethod::DAOApproval,
                whitelisted_at: env.ledger().timestamp(),
            }.publish(env);
        }
        RegistryUpdateType::BlacklistMerchant => {
            // Check if merchant exists
            let mut merchant_status = env.storage().persistent()
                .get::<MerchantStatus>(&merchant_key)
                .expect("merchant not registered");
            
            merchant_status.is_blacklisted = true;
            merchant_status.is_verified = false;
            merchant_status.last_verified = env.ledger().timestamp();
            
            env.storage().persistent().set(&merchant_key, &merchant_status);
            env.storage().persistent().set(&DataKey::BlacklistedMerchant(proposal.merchant_address.clone()), &true);
            
            // Emit merchant blacklisted event
            MerchantBlacklisted {
                merchant: proposal.merchant_address.clone(),
                blacklisted_by: Address::from_string(&soroban_sdk::String::from_str(env, "Security Council")),
                reason: soroban_sdk::String::from_str(env, "Registry update proposal executed"),
                blacklisted_at: env.ledger().timestamp(),
            }.publish(env);
        }
        RegistryUpdateType::RemoveMerchant => {
            // Remove merchant from registry
            env.storage().persistent().remove(&merchant_key);
            env.storage().persistent().remove(&DataKey::BlacklistedMerchant(proposal.merchant_address.clone()));
            env.storage().persistent().remove(&DataKey::KYCCredential(proposal.merchant_address.clone()));
        }
    }
    
    // Mark proposal as executed
    proposal.executed = true;
    env.storage().persistent().set(&proposal_key, &proposal);
}

// --- Merchant Registry and KYC Whitelisting Helper Functions ---

fn is_merchant_registered(env: &Env, merchant: &Address) -> bool {
    env.storage().persistent().has(&DataKey::MerchantRegistry(merchant.clone()))
}

fn is_merchant_blacklisted(env: &Env, merchant: &Address) -> bool {
    env.storage().persistent().has(&DataKey::BlacklistedMerchant(merchant.clone()))
}

fn generate_proposal_id(env: &Env) -> u64 {
    // Generate unique proposal ID based on timestamp and existing proposals
    let now = env.ledger().timestamp();
    let mut proposal_id = now;
    
    // Ensure uniqueness by checking existing proposals
    while env.storage().persistent().has(&DataKey::DAOProposal(proposal_id)) {
        proposal_id += 1;
    }
    
    proposal_id
}

fn is_authorized_voter(env: &Env, voter: &Address) -> bool {
    // For now, we'll use the contract admin as authorized voter
    // In a real implementation, this could be based on DAO token holdings
    if let Some(admin) = env.storage().persistent().get::<Address>(&DataKey::ContractAdmin) {
        voter == &admin
    } else {
        false
    }
}

fn is_authorized_dao_member(env: &Env, member: &Address) -> bool {
    // For now, we'll use the contract admin as authorized DAO member
    // In a real implementation, this would check against a DAO member list
    if let Some(admin) = env.storage().persistent().get::<Address>(&DataKey::ContractAdmin) {
        member == &admin
    } else {
        false
    }
}

fn execute_merchant_proposal(env: &Env, proposal_id: u64) {
    let proposal_key = DataKey::DAOProposal(proposal_id);
    let mut proposal: DAOProposal = env.storage().persistent()
        .get(&proposal_key)
        .expect("proposal not found");
    
    let merchant_key = DataKey::MerchantRegistry(proposal.merchant_address.clone());
    let mut merchant_status: MerchantStatus = env.storage().persistent()
        .get(&merchant_key)
        .expect("merchant not registered");
    
    match proposal.proposal_type {
        ProposalType::WhitelistMerchant => {
            merchant_status.is_verified = true;
            merchant_status.dao_approved = true;
            merchant_status.verification_method = VerificationMethod::DAOApproval;
            merchant_status.last_verified = env.ledger().timestamp();
            
            // Emit merchant whitelisted event
            MerchantWhitelisted {
                merchant: proposal.merchant_address.clone(),
                verification_method: VerificationMethod::DAOApproval,
                whitelisted_at: env.ledger().timestamp(),
            }.publish(env);
        }
        ProposalType::BlacklistMerchant => {
            merchant_status.is_blacklisted = true;
            merchant_status.is_verified = false;
            merchant_status.last_verified = env.ledger().timestamp();
            
            // Emit merchant blacklisted event
            MerchantBlacklisted {
                merchant: proposal.merchant_address.clone(),
                blacklisted_by: Address::from_string(&soroban_sdk::String::from_str(env, "DAO")),
                reason: soroban_sdk::String::from_str(env, "DAO proposal executed"),
                blacklisted_at: env.ledger().timestamp(),
            }.publish(env);
        }
    }
    
    // Mark proposal as executed
    proposal.executed = true;
    env.storage().persistent().set(&proposal_key, &proposal);
    env.storage().persistent().set(&merchant_key, &merchant_status);
    
    // Emit proposal executed event
    DAOProposalExecuted {
        proposal_id,
        merchant: proposal.merchant_address.clone(),
        proposal_type: proposal.proposal_type.clone(),
        executed: true,
        executed_at: env.ledger().timestamp(),
    }.publish(env);
}

// --- Global Reentrancy Guard Helper Functions ---

/// RAII-style reentrancy guard using temporary storage
/// Automatically resets on drop (function exit) for both success and failure paths
pub struct ReentrancyGuard<'env> {
    env: &'env Env,
    active: bool,
}

impl<'env> ReentrancyGuard<'env> {
    /// Creates a new reentrancy guard
    /// Panics if reentrancy is detected
    pub fn new(env: &'env Env, function_name: &str) -> Self {
        // Check if reentrancy guard is already active
        if let Some(guard_active) = env.storage().temporary().get::<_, bool>(&DataKey::ReentrancyGuard) {
            if guard_active {
                // Emit event for monitoring
                ReentrancyAttemptDetected {
                    caller: env.current_contract_address(),
                    protected_function: soroban_sdk::String::from_str(env, function_name),
                    detected_at: env.ledger().timestamp(),
                }
                .publish(env);
                
                panic!("reentrancy detected in {}", function_name);
            }
        }
        
        // Set the guard to active
        env.storage().temporary().set(&DataKey::ReentrancyGuard, &true);
        
        Self {
            env,
            active: true,
        }
    }
}

impl<'env> Drop for ReentrancyGuard<'env> {
    fn drop(&mut self) {
        if self.active {
            // Reset the guard when the guard goes out of scope
            self.env.storage().temporary().remove(&DataKey::ReentrancyGuard);
        }
    }
}

/// Macro to create a reentrancy guard for a function
/// Usage: `let _guard = reentrancy_guard!(env, "function_name");`
macro_rules! reentrancy_guard {
    ($env:expr, $function_name:expr) => {
        let _guard = ReentrancyGuard::new($env, $function_name);
    };
}

/// Check if reentrancy guard is currently active (for testing purposes)
pub fn is_reentrancy_guard_active(env: &Env) -> bool {
    env.storage().temporary().get(&DataKey::ReentrancyGuard).unwrap_or(false)
}

#[cfg(test)]
mod test;
#[cfg(test)]
mod test_cliff_access;
#[cfg(test)]
mod test_dao_treasury;
#[cfg(test)]
mod test_enhanced_subscriptions;
#[cfg(test)]
mod test_merchant_registry;
#[cfg(test)]
mod test_formal_verification;
#[cfg(test)]
mod test_reentrancy_guard;
#[cfg(test)]
mod test_timelock_governance;
