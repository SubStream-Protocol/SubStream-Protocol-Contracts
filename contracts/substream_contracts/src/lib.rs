#![no_std]
#[cfg(test)]
extern crate std;

mod billing_dispute;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::{contract, contractevent, contractimpl, contracttype, vec, Address, Env};

// --- Constants ---

// --- Issue #136: Subscription struct bitmask flags ---
/// Bitmask flag: set when the free-trial-to-paid conversion event has been emitted.
/// Bit 0 of `Subscription::flags`.
pub const FLAGS_FREE_TO_PAID: u8 = 0x01;

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
const MAX_LOYALTY_DISCOUNT_PERIODS: u64 = 10; // Max 50% discount (10 * 5%)
const REFERRAL_REBATE_BPS: i128 = 100; // 1% rebate
const TTL_THRESHOLD: u32 = 17280; // Assuming ~1 day in ledgers for example
const TTL_BUMP_AMOUNT: u32 = 518400; // Assuming ~30 days in ledgers for example

// --- SLA Circuit Breaker Constants ---
const SLA_THRESHOLD_BPS: u32 = 9990; // 99.9% uptime threshold (in basis points)
const SEVEN_DAYS: u64 = 7 * 24 * 60 * 60;
const UPTIME_ORACLE_NONCE_TTL: u64 = 24 * 60 * 60; // 24 hour validity for oracle signatures

// --- Cancel Velocity Circuit Breaker Constants ---
const DAY_IN_SECONDS: u64 = 24 * 60 * 60;
const HOUR_IN_SECONDS: u64 = 60 * 60;
const CANCEL_VELOCITY_HOURLY_BUCKETS: u32 = 24;
const CANCEL_VELOCITY_DAILY_BUCKETS: u32 = 30;
const CANCEL_VELOCITY_MULTIPLIER: u32 = 5; // 500% of 30d average
const CANCEL_VELOCITY_MIN_TRIGGER: u32 = 25; // Ignore small protocols / cold start noise

// --- Merchant Registry and KYC Whitelisting Constants ---
pub(crate) const DAO_MULTISIG_THRESHOLD: u32 = 3; // Minimum signatures required for DAO decisions
const MERCHANT_KYC_VALIDITY: u64 = 365 * 24 * 60 * 60; // 1 year validity for KYC credentials
const SEP12_KYC_ISSUER: &str = "GD5DQX2K7Q4D4PE4R6J4Y7Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2"; // SEP-12 KYC issuer address
const TIMELOCK_DURATION: u64 = 48 * 60 * 60; // 48-hour timelock for registry updates
const SECURITY_COUNCIL_SIZE: u32 = 5; // 5-member security council for multi-sig

// --- Subscription billing / dispute escrow ---
pub(crate) const DISPUTE_WINDOW_SEC: u64 = 48 * 60 * 60;

// --- Helper: Charge Calculation ---
fn calculate_discounted_charge(
    streak_start_date: u64,
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
        let elapsed_since_start = current_t.saturating_sub(streak_start_date);
        let periods = elapsed_since_start / SIX_MONTHS;
        let capped_periods = periods.min(MAX_LOYALTY_DISCOUNT_PERIODS);
        let percent_discount = capped_periods * 5;
        let discount = if percent_discount > 100 {
            100
        } else {
            percent_discount
        };

        let current_rate = base_rate * (100 - discount as i128) / 100;

        let next_boundary = streak_start_date + (periods + 1) * SIX_MONTHS;
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
    Subscription(Address, Address),       // (subscriber, stream_id)
    CliffThreshold(Address),
    CreatorSubscribers(Address),
    CreatorMetadata(Address),
    CreatorAudience(Address, Address),     // (creator, fan)
    ChannelPaused(Address),
    Escrow(Address, Address),
    Nullifier(Bytes),
    NullifierExpirationIndex(u64),         // Index for tracking nullifier expiration cleanup
    YieldConfig(Address),
    SLAStatus(Address),                    // Creator's SLA status
    UptimeOracleNonce(u64),               // Oracle nonce tracking
    ContractAdmin,                         // Integrated for verify_creator
    VerifiedCreator(Address),
    UserReferrer(Address),
    ReferralTracker(Address, Address),
    CurrentFlowRate(Address),          // Aggregated flow rate for a channel
    AcceptedToken(Address),            // Issue #49: Creator's enforced stablecoin token
    DaoGrant(Address, Address),        // (dao, creator) — DAO treasury grant stream
    UserContributed(Address, Address), // (fan, creator) — lifetime tokens contributed by fan
    TopFans(Address),                  // (creator) — Top 50 fans by contribution
    CancelVelocityHourlyBuckets,
    CancelVelocityDailyBuckets,
    CancelVelocityBreakerState,
    // Issue #129: subscriber → ordered list of creator addresses for pagination
    SubscriberIndex(Address),
    // Issue #134: tombstone left after pruning a stale canceled subscription
    Tombstone(Address, Address),       // (subscriber, creator)
    // Issue #134: canceled subscription record (subscriber, creator) → CanceledRecord
    CanceledRecord(Address, Address),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TopFan {
    pub fan: Address,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Tier {
    pub rate_per_second: i128,
    pub trial_duration: u64,
}

/// # Issue #136 – Gas Optimization: Packed Subscription Struct
///
/// Fields are ordered **largest-to-smallest** to minimise XDR padding bytes,
/// reducing Soroban persistent-storage rent and per-call execution cost.
///
/// ## Bitmask schema for `flags` (`u8`)
/// | Bit | Constant            | Meaning                                      |
/// |-----|---------------------|----------------------------------------------|
/// | 0   | `FLAGS_FREE_TO_PAID`| Trial-to-paid conversion event already emitted |
///
/// Use bitwise helpers instead of the removed `free_to_paid_emitted` field:
/// ```rust
/// // read
/// let emitted = sub.flags & FLAGS_FREE_TO_PAID != 0;
/// // set
/// sub.flags |= FLAGS_FREE_TO_PAID;
/// ```
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Subscription {
    // --- i128 fields (16 bytes each) ---
    pub balance: i128,
    pub accrued_remainder: i128, // Dust/fractional units that haven't been paid as tokens
    // --- u64 fields (8 bytes each) ---
    pub last_collected: u64,
    pub start_time: u64,
    pub streak_start_date: u64, // Track original start for loyalty rewards
    pub last_funds_exhausted: u64,
    // --- variable-length / pointer-sized fields ---
    pub token: Address,
    pub tier: Tier,
    pub creators: soroban_sdk::Vec<Address>,
    pub percentages: soroban_sdk::Vec<u32>,
    pub payer: Address,
    pub beneficiary: Address,
    // --- u8 bitmask (replaces individual bool fields) ---
    /// Packed boolean flags – see `FLAGS_*` constants for bit definitions.
    pub flags: u8,
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
pub struct HourlyCancelBucket {
    pub hour_epoch: u64,
    pub count: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DailyCancelBucket {
    pub day_epoch: u64,
    pub count: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VelocityCircuitBreakerState {
    pub active: bool,
    pub soft_pause_active: bool,
    pub triggered_at: u64,
    pub last_updated: u64,
    pub last_velocity: u32,
    pub last_threshold: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancelVelocityMetrics {
    pub rolling_24h_cancellations: u32,
    pub trailing_30d_cancellations: u32,
    pub daily_average_30d: u32,
    pub anomaly_threshold: u32,
    pub circuit_breaker_active: bool,
    pub soft_pause_active: bool,
    pub triggered_at: u64,
    pub hourly_bucket_count: u32,
    pub daily_bucket_count: u32,
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

/// Usage-based (pay-as-you-go) price components registered by the merchant for a plan.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicPlan {
    pub base_fee: i128,
    pub per_unit_rate: i128,
}

/// Snapshot of dynamic pricing and the subscriber-approved per-pull cap.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicBillingInfo {
    pub base_fee: i128,
    pub per_unit_rate: i128,
    pub maximum_billing_cap: i128,
    pub plan_id: u32,
}

/// Signed payload: `units_consumed` and `usage_timestamp` are covered by `signature`
/// over [`dynamic_usage_attestation_message`].
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicUsageOraclePayload {
    pub subscriber: Address,
    pub merchant: Address,
    pub units_consumed: i128,
    pub usage_timestamp: u64,
    pub nonce: u64,
    pub signature: BytesN<64>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SubscriptionStatus {
    Active,
    PastDue,
    Canceled,
    Trial,
    /// Pulls paused; last billed amount held in dispute escrow pending juror verdict.
    Disputed,
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

/// Snapshot of the last executed pull (used for the 48h dispute window).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingMerchantPullInfo {
    pub amount: i128,
    pub token: Address,
    pub pulled_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeRecord {
    pub dispute_id: u64,
    pub subscriber: Address,
    pub merchant: Address,
    pub disputed_amount: i128,
    pub bond_amount: i128,
    pub token: Address,
    pub raised_at: u64,
    pub resolved: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JurorSignature {
    pub pubkey: BytesN<32>,
    pub sig: BytesN<64>,
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

// --- Issue #124: DAO Treasury Token Buyback Hook ---

/// Global configuration for the DAO treasury buyback hook.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuybackConfig {
    /// DAO treasury wallet that accumulates protocol revenue.
    pub dao_treasury: Address,
    /// DEX router contract address (cross-contract call target).
    pub dex_router: Address,
    /// Governance token to buy back with protocol revenue.
    pub governance_token: Address,
    /// Payment token used to buy the governance token (e.g. USDC).
    pub payment_token: Address,
    /// Minimum protocol revenue (in payment_token units) before buyback fires.
    pub trigger_threshold: i128,
    /// Hard-coded gas bounty paid to the relayer that triggers the buyback.
    pub relayer_bounty: i128,
    /// Maximum slippage tolerance in basis-points (e.g. 50 = 0.5%).
    pub max_slippage_bps: u32,
    /// Whether the buyback hook is enabled.
    pub enabled: bool,
}

/// Record of a completed buyback operation.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuybackRecord {
    pub nonce: u64,
    pub triggered_by: Address,
    pub payment_amount: i128,
    pub governance_tokens_acquired: i128,
    pub bounty_paid: i128,
    pub executed_at: u64,
}

// --- Issue #125: Merchant Terms of Service IPFS Anchoring ---

/// On-chain ToS anchor — stores a content-addressed IPFS CID.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MerchantToSAnchor {
    /// Merchant that published this ToS.
    pub merchant: Address,
    /// IPFS CIDv1 hash of the Terms of Service document (max 64 bytes).
    pub ipfs_hash: soroban_sdk::Bytes,
    /// Monotonically increasing version counter, starting at 1.
    pub version: u32,
    /// Block timestamp when this ToS was anchored.
    pub anchored_at: u64,
}

/// Snapshot stored in the subscription record at subscribe-time.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToSSnapshot {
    /// IPFS hash that was active when the subscriber agreed.
    pub ipfs_hash: soroban_sdk::Bytes,
    /// Version number of the agreed ToS.
    pub version: u32,
    /// Timestamp of agreement.
    pub agreed_at: u64,
}

// --- Issue #128: Merchant Metrics ---

/// Aggregated on-chain metrics for a merchant, queryable in one call.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MerchantMetrics {
    /// Total number of subscribers that have ever subscribed.
    pub total_subscribers: u64,
    /// Currently active subscribers (subscription not cancelled / expired).
    pub active_subscribers: u64,
    /// Subscribers currently in the dunning / grace-period window.
    pub dunning_subscribers: u64,
    /// Gross revenue collected by this merchant (in token units).
    pub total_revenue: i128,
    /// Average revenue per active subscriber (recomputed on every update).
    pub avg_revenue_per_subscriber: i128,
    /// Timestamp of the last time these metrics were updated.
    pub last_updated: u64,
}

// --- Issue #121: Multi-Sig Family Shared Allowances ---

/// Multi-signature vault configuration for family/team treasury management.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FamilyVaultConfig {
    /// Vault owner/creator address.
    pub owner: Address,
    /// List of authorized signer addresses for multi-sig operations.
    pub signers: soroban_sdk::Vec<Address>,
    /// Minimum number of signatures required (threshold).
    pub threshold: u32,
    /// Total allowance allocated for subscription spending.
    pub allowance: i128,
    /// Amount already spent from allowance.
    pub spent: i128,
    /// Token used for the vault allowance.
    pub token: Address,
    /// Whether the vault is active.
    pub is_active: bool,
    /// Timestamp when vault was created.
    pub created_at: u64,
}

/// Delegate authorization for subscription-only spending from vault.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultDelegate {
    /// Delegate address authorized to manage subscriptions.
    pub delegate: Address,
    /// Vault this delegate is authorized for.
    pub vault_id: Address,
    /// Whether delegate can only subscribe (not withdraw).
    pub subscription_only: bool,
    /// Maximum spending limit for this delegate.
    pub spending_limit: i128,
    /// Amount spent by this delegate.
    pub amount_spent: i128,
    /// Authorization expiry timestamp.
    pub expires_at: u64,
}

// --- Issue #122: Vacation Mode ---

/// Merchant vacation mode status for temporary service pause.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VacationModeStatus {
    /// Whether vacation mode is currently active.
    pub is_active: bool,
    /// Timestamp when vacation mode was activated.
    pub activated_at: u64,
    /// Timestamp when vacation mode was deactivated (0 if still active).
    pub deactivated_at: u64,
    /// Total duration of vacation pause in seconds.
    pub pause_duration: u64,
}

// --- Issue #123: Affiliate Referral Fee Routing ---

/// Affiliate configuration for a merchant's referral program.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AffiliateConfig {
    /// Commission rate in basis points (e.g., 1000 = 10%).
    pub commission_bps: u32,
    /// Whether affiliate program is enabled.
    pub is_enabled: bool,
    /// Minimum payout threshold for affiliates.
    pub min_payout: i128,
    /// Total commissions paid out.
    pub total_paid: i128,
}

/// Affiliate referral tracking for a specific affiliate-merchant pair.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AffiliateReferralInfo {
    /// Affiliate address.
    pub affiliate: Address,
    /// Number of successful referrals.
    pub referral_count: u32,
    /// Total commissions earned (pending + claimed).
    pub total_earned: i128,
    /// Amount already claimed/paid out.
    pub total_claimed: i128,
    /// Last payout timestamp.
    pub last_payout_at: u64,
}

// --- Issue #131: Allowance Health Pre-flight Check ---

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AllowanceHealth {
    Healthy,
    InsufficientFunds,
    AllowanceRevoked,
}

// --- Issue #134: Canceled Subscription Record for Pruning ---

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanceledRecord {
    pub token: Address,
    pub canceled_at: u64,
}

// --- Issue #134: StaleDataPruned event ---

#[contractevent]
pub struct StaleDataPruned {
    #[topic] pub subscriber: Address,
    #[topic] pub creator: Address,
    pub tombstone: soroban_sdk::Bytes,
    pub pruned_at: u64,
}

// --- Issue #129: Paginated subscription result ---

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubscriptionPage {
    pub items: soroban_sdk::Vec<Subscription>,
    pub next_cursor: u32,
    pub has_more: bool,
}



#[contractevent]
pub struct FamilyVaultCreated {
    #[topic] pub vault_id: Address,
    #[topic] pub owner: Address,
    pub threshold: u32,
    pub allowance: i128,
    pub created_at: u64,
}

#[contractevent]
pub struct DelegateAuthorized {
    #[topic] pub vault_id: Address,
    #[topic] pub delegate: Address,
    pub spending_limit: i128,
    pub authorized_at: u64,
}

#[contractevent]
pub struct DelegateRevoked {
    #[topic] pub vault_id: Address,
    #[topic] pub delegate: Address,
    pub revoked_at: u64,
}

#[contractevent]
pub struct VacationModeActivated {
    #[topic] pub merchant: Address,
    pub activated_at: u64,
}

#[contractevent]
pub struct VacationModeDeactivated {
    #[topic] pub merchant: Address,
    pub deactivated_at: u64,
    pub total_pause_duration: u64,
}

#[contractevent]
pub struct AffiliateConfigured {
    #[topic] pub merchant: Address,
    pub commission_bps: u32,
    pub configured_at: u64,
}

#[contractevent]
pub struct AffiliateReferralRecorded {
    #[topic] pub merchant: Address,
    #[topic] pub affiliate: Address,
    #[topic] pub referred_user: Address,
    pub commission_amount: i128,
    pub recorded_at: u64,
}

#[contractevent]
pub struct AffiliatePayoutClaimed {
    #[topic] pub merchant: Address,
    #[topic] pub affiliate: Address,
    pub payout_amount: i128,
    pub claimed_at: u64,
}

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
pub struct VelocityAnomalyDetected {
    pub current_velocity: u32,
    pub threshold: u32,
    pub timestamp: u64,
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
pub struct DisputeRaised {
    #[topic] pub dispute_id: u64,
    #[topic] pub subscriber: Address,
    #[topic] pub merchant: Address,
    pub disputed_amount: i128,
    pub bond_amount: i128,
    pub raised_at: u64,
}

#[contractevent]
pub struct DisputeResolved {
    #[topic] pub dispute_id: u64,
    #[topic] pub subscriber: Address,
    #[topic] pub merchant: Address,
    pub user_wins: bool,
    pub refunded_to_user: i128,
    pub paid_to_merchant: i128,
    pub bond_destination: Address,
    pub bond_amount: i128,
    pub resolved_at: u64,
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

// --- Issue #133: Analytics Events ---

/// Emitted when a subscription upgrade triggers proration math.
/// Provides marketing/analytics teams with the exact unused value and new tier cost.
#[contractevent]
pub struct ProrationCalculated {
    #[topic] pub subscriber: Address,
    #[topic] pub merchant: Address,
    pub unused_value: i128,
    pub new_tier_cost: i128,
    pub calculated_at: u64,
}

/// Emitted the first time a trial subscription converts to a paid billing cycle.
/// `is_first_payment` is `true` when this is the subscriber's very first payment
/// to this merchant (i.e. the trial was never previously converted).
#[contractevent]
pub struct TrialAutoConverted {
    #[topic] pub subscriber: Address,
    #[topic] pub merchant: Address,
    pub is_first_payment: bool,
    pub converted_at: u64,
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

// --- Issue #124: DAO Treasury Buyback Events ---

#[contractevent]
pub struct BuybackConfigured {
    #[topic] pub dao_treasury: Address,
    #[topic] pub governance_token: Address,
    pub dex_router: Address,
    pub payment_token: Address,
    pub trigger_threshold: i128,
    pub relayer_bounty: i128,
    pub max_slippage_bps: u32,
    pub configured_at: u64,
}

#[contractevent]
pub struct BuybackTriggered {
    #[topic] pub relayer: Address,
    #[topic] pub nonce: u64,
    pub payment_amount: i128,
    pub governance_tokens_acquired: i128,
    pub bounty_paid: i128,
    pub executed_at: u64,
}

#[contractevent]
pub struct BuybackNonceCommitted {
    #[topic] pub nonce: u64,
    #[topic] pub committed_by: Address,
    pub committed_at: u64,
}

// --- Issue #125: ToS Anchoring Events ---

#[contractevent]
pub struct ToSAnchored {
    #[topic] pub merchant: Address,
    #[topic] pub version: u32,
    pub ipfs_hash: soroban_sdk::Bytes,
    pub anchored_at: u64,
}

#[contractevent]
pub struct ToSAgreed {
    #[topic] pub subscriber: Address,
    #[topic] pub merchant: Address,
    pub tos_version: u32,
    pub ipfs_hash: soroban_sdk::Bytes,
    pub agreed_at: u64,
}

// --- Issue #126: Standardized Protocol Events ---
// (These events standardize all existing lifecycle state transitions)

#[contractevent]
pub struct SubscriptionCreated {
    #[topic] pub subscriber: Address,
    #[topic] pub merchant: Address,
    #[topic] pub plan_id: u32,
    pub token: Address,
    pub rate_per_second: i128,
    pub created_at: u64,
}

#[contractevent]
pub struct SubscriptionCancelled {
    #[topic] pub subscriber: Address,
    #[topic] pub merchant: Address,
    pub refund_amount: i128,
    pub cancelled_at: u64,
}

#[contractevent]
pub struct SubscriptionRenewed {
    #[topic] pub subscriber: Address,
    #[topic] pub merchant: Address,
    #[topic] pub amount: i128,
    pub next_billing_date: u64,
    pub renewed_at: u64,
}

#[contractevent]
pub struct MerchantRegistered {
    #[topic] pub merchant: Address,
    #[topic] pub verification_method: VerificationMethod,
    pub registered_at: u64,
}

#[contractevent]
pub struct PlanRegistered {
    #[topic] pub merchant: Address,
    #[topic] pub plan_id: u32,
    pub billing_amount: i128,
    pub billing_cycle: u64,
    pub registered_at: u64,
}

#[contractevent]
pub struct ProtocolRevenueCollected {
    #[topic] pub merchant: Address,
    #[topic] pub token: Address,
    pub fee_amount: i128,
    pub collected_at: u64,
}

// --- Issue #128: Merchant Metrics Events ---

#[contractevent]
pub struct MerchantMetricsUpdated {
    #[topic] pub merchant: Address,
    pub active_subscribers: u64,
    pub dunning_subscribers: u64,
    pub total_revenue: i128,
    pub updated_at: u64,
}

/// Macro to create a reentrancy guard for a function
/// Usage: `let _guard = reentrancy_guard!(env, "function_name");`
macro_rules! reentrancy_guard {
    ($env:expr, $function_name:expr) => {
        let _guard = ReentrancyGuard::new($env, $function_name);
    };
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
        require_protocol_soft_pause_inactive(env);

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
            sub.streak_start_date,
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
        require_protocol_soft_pause_inactive(&env);
        top_up_internal(&env, &subscriber, &stream_id, amount);
    }

    pub fn cancel(env: Env, subscriber: Address, creator: Address) {
        cancel_internal(&env, &subscriber, &creator);
    }

    pub fn get_cancel_velocity_metrics(env: Env) -> CancelVelocityMetrics {
        sync_cancel_velocity_metrics(&env)
    }

    pub fn is_protocol_soft_paused(env: Env) -> bool {
        read_velocity_circuit_breaker_state(&env).soft_pause_active
    }

    pub fn reset_cancel_velocity_circuit_breaker(env: Env, admin: Address) {
        admin.require_auth();
        require_contract_admin(&env, &admin);

        let now = env.ledger().timestamp();
        let mut state = read_velocity_circuit_breaker_state(&env);
        state.active = false;
        state.soft_pause_active = false;
        state.triggered_at = 0;
        state.last_updated = now;
        write_velocity_circuit_breaker_state(&env, &state);
    }

    pub fn tip(env: Env, user: Address, creator: Address, token: Address, amount: i128) {
        require_protocol_soft_pause_inactive(&env);
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
        require_protocol_soft_pause_inactive(&env);

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

    /// Returns the top 50 fans for a creator by total lifetime contributions.
    pub fn get_top_fans(env: Env, creator: Address) -> soroban_sdk::Vec<TopFan> {
        env.storage()
            .persistent()
            .get(&DataKey::TopFans(creator))
            .unwrap_or(soroban_sdk::Vec::new(&env))
    }

    // -----------------------------------------------------------------------
    // Multi-Tier Subscription Upgrade
    // -----------------------------------------------------------------------
    pub fn upgrade_subscription(
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
        
        // Issue #133: emit ProrationCalculated analytics event
        ProrationCalculated {
            subscriber: subscriber.clone(),
            merchant: merchant.clone(),
            unused_value,
            new_tier_cost: new_plan.billing_amount,
            calculated_at: now,
        }.publish(&env);
        
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
            old_tier_id: old_plan_id,
            new_tier_id,
            prorated_charge,
            upgraded_at: now,
        }.publish(&env);
    }
    
    // Helper function for merchants to register plans
    pub fn register_plan(env: Env, merchant: Address, plan: Plan) {
        require_protocol_soft_pause_inactive(&env);
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

        // Issue #126: emit standardized PlanRegistered event
        let now = env.ledger().timestamp();
        PlanRegistered {
            merchant: merchant.clone(),
            plan_id: plan.plan_id,
            billing_amount: plan.billing_amount,
            billing_cycle: plan.billing_cycle,
            registered_at: now,
        }
        .publish(&env);
        
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

        // Issue #126: standardized MerchantRegistered event for subgraph parity
        MerchantRegistered {
            merchant,
            verification_method: VerificationMethod::SEP12KYC,
            registered_at: now,
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

    // =========================================================================
    // Issue #124: Native DAO Treasury Token Buyback Hook
    // =========================================================================

    /// Admin configures the buyback hook parameters.
    /// Only the contract admin may call this.
    pub fn configure_buyback(
        env: Env,
        admin: Address,
        dao_treasury: Address,
        dex_router: Address,
        governance_token: Address,
        payment_token: Address,
        trigger_threshold: i128,
        relayer_bounty: i128,
        max_slippage_bps: u32,
    ) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::ContractAdmin)
            .expect("not initialized");
        if admin != stored_admin {
            panic!("admin only");
        }
        if trigger_threshold <= 0 {
            panic!("trigger threshold must be positive");
        }
        if relayer_bounty < 0 {
            panic!("relayer bounty cannot be negative");
        }
        if max_slippage_bps > 10000 {
            panic!("slippage bps exceeds 100%");
        }
        let now = env.ledger().timestamp();
        let config = BuybackConfig {
            dao_treasury: dao_treasury.clone(),
            dex_router: dex_router.clone(),
            governance_token: governance_token.clone(),
            payment_token: payment_token.clone(),
            trigger_threshold,
            relayer_bounty,
            max_slippage_bps,
            enabled: true,
        };
        env.storage().persistent().set(&DataKey::BuybackConfig, &config);
        BuybackConfigured {
            dao_treasury,
            governance_token,
            dex_router,
            payment_token,
            trigger_threshold,
            relayer_bounty,
            max_slippage_bps,
            configured_at: now,
        }
        .publish(&env);
    }

    /// Commit a nonce before triggering a buyback to prevent front-running.
    /// Relayer submits `hash(nonce || relayer_address)` to anchor its intent.
    pub fn commit_buyback_nonce(env: Env, relayer: Address, nonce: u64) {
        relayer.require_auth();
        let key = DataKey::BuybackNonce(nonce);
        if env.storage().persistent().has(&key) {
            panic!("nonce already committed");
        }
        let now = env.ledger().timestamp();
        // Store relayer address so only they can execute with this nonce
        env.storage().persistent().set(&key, &relayer);
        BuybackNonceCommitted {
            nonce,
            committed_by: relayer,
            committed_at: now,
        }
        .publish(&env);
    }

    /// Trigger the buyback: transfer protocol revenue from the DAO treasury
    /// into governance tokens via the registered DEX router.
    /// The relayer that committed the nonce receives a small gas bounty.
    /// Front-running protection: only the address that committed `nonce` may execute.
    pub fn trigger_buyback(env: Env, relayer: Address, nonce: u64, min_tokens_out: i128) {
        relayer.require_auth();
        // Reentrancy guard
        let _guard = reentrancy_guard!(&env, "trigger_buyback");

        // --- Validate nonce ownership (front-run protection) ---
        let nonce_key = DataKey::BuybackNonce(nonce);
        let committed_relayer: Address = env
            .storage()
            .persistent()
            .get(&nonce_key)
            .expect("nonce not committed");
        if committed_relayer != relayer {
            panic!("nonce belongs to different relayer");
        }
        // Burn the nonce immediately to prevent replay
        env.storage().persistent().remove(&nonce_key);

        // --- Load and validate config ---
        let config: BuybackConfig = env
            .storage()
            .persistent()
            .get(&DataKey::BuybackConfig)
            .expect("buyback not configured");
        if !config.enabled {
            panic!("buyback is disabled");
        }

        let payment_token_client = TokenClient::new(&env, &config.payment_token);
        let treasury_balance = payment_token_client.balance(&config.dao_treasury);

        if treasury_balance < config.trigger_threshold {
            panic!("treasury balance below trigger threshold");
        }

        // Amount to swap = full treasury balance minus bounty reserve
        let swap_amount = treasury_balance.saturating_sub(config.relayer_bounty).max(0);
        if swap_amount <= 0 {
            panic!("insufficient balance after bounty reserve");
        }

        // --- Validate slippage: min_tokens_out must be within max_slippage ---
        // We enforce at the call level; the DEX router enforces on-chain.
        // min_tokens_out = 0 is only acceptable in tests (slippage = 100%)
        // For production, caller must supply a realistic floor.
        let _ = min_tokens_out; // passed to DEX router call below

        // --- Cross-contract call to DEX router ---
        // Interface: dex_router.swap(payment_token, governance_token, amount_in, min_amount_out)
        // We model this as a token transfer from treasury -> contract, then router call.
        // In practice the router receives approval and swaps atomically.
        payment_token_client.transfer(&config.dao_treasury, &env.current_contract_address(), &swap_amount);

        // Simulate DEX swap: in a real deployment the router contract is called here.
        // For now we transfer the swap_amount to the dex_router and record what we expect back.
        // The governance tokens are credited to the DAO treasury.
        payment_token_client.transfer(&env.current_contract_address(), &config.dex_router, &swap_amount);
        // Record acquired amount (router would return actual amount; we use min_tokens_out as floor).
        let governance_tokens_acquired = min_tokens_out.max(0);

        // --- Pay relayer bounty ---
        let actual_bounty = config.relayer_bounty.min(treasury_balance - swap_amount);
        if actual_bounty > 0 {
            payment_token_client.transfer(&config.dao_treasury, &relayer, &actual_bounty);
        }

        let now = env.ledger().timestamp();
        BuybackTriggered {
            relayer,
            nonce,
            payment_amount: swap_amount,
            governance_tokens_acquired,
            bounty_paid: actual_bounty,
            executed_at: now,
        }
        .publish(&env);
    }

    /// Read-only query for the current buyback configuration.
    pub fn get_buyback_config(env: Env) -> BuybackConfig {
        env.storage()
            .persistent()
            .get(&DataKey::BuybackConfig)
            .expect("buyback not configured")
    }

    // =========================================================================
    // Issue #125: Anchoring Merchant Terms of Service (IPFS Hashes)
    // =========================================================================

    /// Merchant anchors a new version of their Terms of Service on-chain.
    /// `ipfs_hash` must be a valid IPFS CID encoded as bytes (max 64 bytes).
    /// Each call increments the version counter — old versions remain readable
    /// via off-chain event log but subscribers' snapshots remain immutable.
    pub fn anchor_merchant_tos(
        env: Env,
        merchant: Address,
        ipfs_hash: soroban_sdk::Bytes,
    ) {
        merchant.require_auth();
        // Verify merchant is registered
        if !is_merchant_verified(&env, &merchant) {
            panic!("merchant is not verified");
        }
        if ipfs_hash.len() == 0 {
            panic!("ipfs hash cannot be empty");
        }
        if ipfs_hash.len() > 64 {
            panic!("ipfs hash too long (max 64 bytes)");
        }

        // Increment version
        let version_key = DataKey::MerchantToSVersion(merchant.clone());
        let new_version: u32 = env
            .storage()
            .persistent()
            .get::<u32>(&version_key)
            .unwrap_or(0)
            .saturating_add(1);

        let now = env.ledger().timestamp();
        let anchor = MerchantToSAnchor {
            merchant: merchant.clone(),
            ipfs_hash: ipfs_hash.clone(),
            version: new_version,
            anchored_at: now,
        };

        env.storage().persistent().set(&DataKey::MerchantToS(merchant.clone()), &anchor);
        env.storage().persistent().set(&version_key, &new_version);

        ToSAnchored {
            merchant,
            version: new_version,
            ipfs_hash,
            anchored_at: now,
        }
        .publish(&env);
    }

    /// Returns the current active ToS anchor for a merchant.
    pub fn get_merchant_tos(env: Env, merchant: Address) -> MerchantToSAnchor {
        env.storage()
            .persistent()
            .get(&DataKey::MerchantToS(merchant))
            .expect("no ToS anchored for merchant")
    }

    /// Returns the ToS version number active at subscription time for a given subscriber.
    /// Returns `None` if no snapshot exists (e.g., merchant had no ToS when subscribed).
    pub fn get_subscription_tos_snapshot(
        env: Env,
        subscriber: Address,
        merchant: Address,
    ) -> Option<ToSSnapshot> {
        env.storage()
            .persistent()
            .get(&DataKey::SubscriptionToSSnapshot(subscriber, merchant))
    }

    /// Verify that a subscriber agreed to the current ToS version.
    /// Returns `true` if the subscriber's snapshot matches the merchant's current ToS.
    pub fn verify_tos_agreement(
        env: Env,
        subscriber: Address,
        merchant: Address,
    ) -> bool {
        let snapshot: Option<ToSSnapshot> = env
            .storage()
            .persistent()
            .get(&DataKey::SubscriptionToSSnapshot(subscriber, merchant.clone()));
        let current: Option<MerchantToSAnchor> = env
            .storage()
            .persistent()
            .get(&DataKey::MerchantToS(merchant));
        match (snapshot, current) {
            (Some(snap), Some(curr)) => snap.version == curr.version,
            _ => false,
        }
    }

    // =========================================================================
    // Issue #128: get_merchant_metrics Read-Only Query
    // =========================================================================

    /// Merchants and dashboards call this to retrieve aggregated business KPIs
    /// in a single read-only invocation. No state mutations occur.
    pub fn get_merchant_metrics(env: Env, merchant: Address) -> MerchantMetrics {
        // Return stored metrics if available (updated by write operations).
        // If no metrics have been recorded yet, return zeroed defaults.
        env.storage()
            .persistent()
            .get(&DataKey::MerchantMetrics(merchant.clone()))
            .unwrap_or(MerchantMetrics {
                total_subscribers: 0,
                active_subscribers: 0,
                dunning_subscribers: 0,
                total_revenue: 0,
                avg_revenue_per_subscriber: 0,
                last_updated: 0,
            })
    }

    /// Internal helper exposed for testing: force-update merchant metrics.
    /// In production, metrics are updated automatically by subscribe / cancel / billing hooks.
    pub fn update_merchant_metrics(
        env: Env,
        caller: Address,
        merchant: Address,
        active_delta: i64,
        dunning_delta: i64,
        revenue_delta: i128,
    ) {
        // Only the merchant or the contract admin may push metric updates.
        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::ContractAdmin)
            .expect("not initialized");
        if caller != stored_admin && caller != merchant {
            panic!("unauthorized metrics update");
        }
        caller.require_auth();

        let key = DataKey::MerchantMetrics(merchant.clone());
        let mut m: MerchantMetrics = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(MerchantMetrics {
                total_subscribers: 0,
                active_subscribers: 0,
                dunning_subscribers: 0,
                total_revenue: 0,
                avg_revenue_per_subscriber: 0,
                last_updated: 0,
            });

        // Apply deltas with saturation to avoid overflow
        if active_delta >= 0 {
            m.active_subscribers = m.active_subscribers.saturating_add(active_delta as u64);
            m.total_subscribers = m.total_subscribers.saturating_add(active_delta as u64);
        } else {
            m.active_subscribers = m.active_subscribers.saturating_sub((-active_delta) as u64);
        }
        if dunning_delta >= 0 {
            m.dunning_subscribers = m.dunning_subscribers.saturating_add(dunning_delta as u64);
        } else {
            m.dunning_subscribers = m.dunning_subscribers.saturating_sub((-dunning_delta) as u64);
        }
        m.total_revenue = m.total_revenue.saturating_add(revenue_delta);
        m.avg_revenue_per_subscriber = if m.active_subscribers > 0 {
            m.total_revenue / m.active_subscribers as i128
        } else {
            0
        };

        let now = env.ledger().timestamp();
        m.last_updated = now;

        env.storage().persistent().set(&key, &m);

        MerchantMetricsUpdated {
            merchant,
            active_subscribers: m.active_subscribers,
            dunning_subscribers: m.dunning_subscribers,
            total_revenue: m.total_revenue,
            updated_at: now,
        }
        .publish(&env);
    }

    // =========================================================================
    // Issue #126: Standardized event emissions for plan registration and
    // subscription lifecycle — complementary helpers
    // =========================================================================

    /// Emit a standardized SubscriptionCreated event for subgraph indexers.
    /// Called internally by subscribe_core; can also be invoked by integrations.
    pub fn emit_subscription_created(
        env: Env,
        subscriber: Address,
        merchant: Address,
        token: Address,
        rate_per_second: i128,
        plan_id: u32,
    ) {
        let now = env.ledger().timestamp();
        SubscriptionCreated {
            subscriber,
            merchant,
            plan_id,
            token,
            rate_per_second,
            created_at: now,
        }
        .publish(&env);
    }

    /// Emit a standardized SubscriptionCancelled event.
    pub fn emit_subscription_cancelled(
        env: Env,
        subscriber: Address,
        merchant: Address,
        refund_amount: i128,
    ) {
        let now = env.ledger().timestamp();
        SubscriptionCancelled {
            subscriber,
            merchant,
            refund_amount,
            cancelled_at: now,
        }
        .publish(&env);
    }

    /// Emit a standardized SubscriptionRenewed event.
    pub fn emit_subscription_renewed(
        env: Env,
        subscriber: Address,
        merchant: Address,
        amount: i128,
        next_billing_date: u64,
    ) {
        let now = env.ledger().timestamp();
        SubscriptionRenewed {
            subscriber,
            merchant,
            amount,
            next_billing_date,
            renewed_at: now,
        }
        .publish(&env);
    }

    /// Emit a standardized PlanRegistered event (called within register_plan).
    pub fn emit_plan_registered(
        env: Env,
        merchant: Address,
        plan_id: u32,
        billing_amount: i128,
        billing_cycle: u64,
    ) {
        let now = env.ledger().timestamp();
        PlanRegistered {
            merchant,
            plan_id,
            billing_amount,
            billing_cycle,
            registered_at: now,
        }
        .publish(&env);
    }

    /// Emit a ProtocolRevenueCollected event when fees are swept.
    pub fn emit_protocol_revenue_collected(
        env: Env,
        merchant: Address,
        token: Address,
        fee_amount: i128,
    ) {
        let now = env.ledger().timestamp();
        ProtocolRevenueCollected {
            merchant,
            token,
            fee_amount,
            collected_at: now,
        }
        .publish(&env);
    }

    // =========================================================================
    // Issue #121: Multi-Sig Family Shared Allowances
    // =========================================================================

    /// Create a new family/team vault with multi-sig control
    if let Some(sub) = env.storage().persistent().get(key) {
        sub
    } else {
        env.storage().temporary().get(key).expect("not found")
    }
}

pub(crate) fn set_subscription(env: &Env, key: &DataKey, sub: &Subscription) {
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

// =========================================================================
// Issue #121: Multi-Sig Family Shared Allowances
// =========================================================================

// These functions have been moved inside the impl block above
    pub fn create_family_vault(
        env: Env,
        vault_id: Address,
        signers: soroban_sdk::Vec<Address>,
        threshold: u32,
        allowance: i128,
        token: Address,
    ) {
        // Vault creator must authenticate
        let creator = signers.get(0).expect("at least one signer required");
        creator.require_auth();

        if signers.len() < threshold {
            panic!("signers count less than threshold");
        }
        if threshold == 0 {
            panic!("threshold must be positive");
        }
        if allowance <= 0 {
            panic!("allowance must be positive");
        }

        let vault_key = DataKey::FamilyVault(vault_id.clone());
        if env.storage().persistent().has(&vault_key) {
            panic!("vault already exists");
        }

        let vault_config = FamilyVaultConfig {
            owner: creator.clone(),
            signers: signers.clone(),
            threshold,
            allowance,
            spent: 0,
            token: token.clone(),
            is_active: true,
            created_at: env.ledger().timestamp(),
        };

        env.storage().persistent().set(&vault_key, &vault_config);

        FamilyVaultCreated {
            vault_id,
            owner: creator,
            threshold,
            allowance,
            created_at: vault_config.created_at,
        }
        .publish(&env);
    }

    /// Authorize a delegate to manage subscriptions from the vault
    pub fn authorize_delegate(
        env: Env,
        vault_id: Address,
        delegate: Address,
        spending_limit: i128,
        expires_at: u64,
    ) {
        // Verify vault exists and get config
        let vault_key = DataKey::FamilyVault(vault_id.clone());
        let mut vault_config: FamilyVaultConfig = env
            .storage()
            .persistent()
            .get(&vault_key)
            .expect("vault not found");

        // Requires multiig authorization - at least threshold signers must approve
        // For simplicity, we require the owner to authorize
        vault_config.owner.require_auth();

        if !vault_config.is_active {
            panic!("vault is not active");
        }
        if spending_limit <= 0 {
            panic!("spending limit must be positive");
        }
        if expires_at <= env.ledger().timestamp() {
            panic!("expiry must be in the future");
        }

        let delegate_config = VaultDelegate {
            delegate: delegate.clone(),
            vault_id: vault_id.clone(),
            subscription_only: true,
            spending_limit,
            amount_spent: 0,
            expires_at,
        };

        env.storage()
            .persistent()
            .set(&DataKey::VaultDelegate(vault_id, delegate.clone()), &delegate_config);

        DelegateAuthorized {
            vault_id,
            delegate,
            spending_limit,
            authorized_at: env.ledger().timestamp(),
        }
        .publish(&env);
    }

    /// Revoke delegate authorization
    pub fn revoke_delegate(env: Env, vault_id: Address, delegate: Address) {
        // Verify vault exists
        let vault_key = DataKey::FamilyVault(vault_id.clone());
        let vault_config: FamilyVaultConfig = env
            .storage()
            .persistent()
            .get(&vault_key)
            .expect("vault not found");

        vault_config.owner.require_auth();

        let delegate_key = DataKey::VaultDelegate(vault_id.clone(), delegate.clone());
        if !env.storage().persistent().has(&delegate_key) {
            panic!("delegate not found");
        }

        env.storage().persistent().remove(&delegate_key);

        DelegateRevoked {
            vault_id,
            delegate,
            revoked_at: env.ledger().timestamp(),
        }
        .publish(&env);
    }

    /// Subscribe to a merchant using vault funds (delegate call)
    pub fn vault_subscribe(
        env: Env,
        vault_id: Address,
        delegate: Address,
        merchant: Address,
        token: Address,
        amount: i128,
        rate_per_second: i128,
    ) {
        delegate.require_auth();

        // Verify delegate is authorized
        let delegate_key = DataKey::VaultDelegate(vault_id.clone(), delegate.clone());
        let mut delegate_config: VaultDelegate = env
            .storage()
            .persistent()
            .get(&delegate_key)
            .expect("delegate not authorized");

        if delegate_config.expires_at < env.ledger().timestamp() {
            panic!("delegate authorization expired");
        }

        // Check spending limit
        let new_spent = delegate_config.amount_spent + amount;
        if new_spent > delegate_config.spending_limit {
            panic!("exceeds delegate spending limit");
        }

        // Verify vault exists and has sufficient allowance
        let vault_key = DataKey::FamilyVault(vault_id.clone());
        let mut vault_config: FamilyVaultConfig = env
            .storage()
            .persistent()
            .get(&vault_key)
            .expect("vault not found");

        if !vault_config.is_active {
            panic!("vault is not active");
        }
        if vault_config.token != token {
            panic!("token mismatch");
        }

        let new_vault_spent = vault_config.spent + amount;
        if new_vault_spent > vault_config.allowance {
            panic!("exceeds vault allowance");
        }

        // Transfer tokens from vault to contract
        let token_client = TokenClient::new(&env, &token);
        token_client.transfer(&delegate, &env.current_contract_address(), &amount);

        // Create subscription
        let sub_key = DataKey::Subscription(delegate.clone(), merchant.clone());
        let now = env.ledger().timestamp();
        let subscription = Subscription {
            token: token.clone(),
            tier: Tier {
                rate_per_second,
                trial_duration: 0,
            },
            balance: amount * PRECISION_MULTIPLIER,
            last_collected: now,
            start_time: now,
            streak_start_date: now,
            last_funds_exhausted: 0,
            flags: 0,
            creators: soroban_sdk::vec![&env, merchant.clone()],
            percentages: soroban_sdk::vec![&env, 100],
            payer: vault_id.clone(),
            beneficiary: delegate.clone(),
            accrued_remainder: 0,
        };

        env.storage().persistent().set(&sub_key, &subscription);

        // Track vault subscription
        env.storage()
            .persistent()
            .set(&DataKey::VaultSubscription(vault_id, merchant), &true);

        // Update spent amounts
        delegate_config.amount_spent = new_spent;
        vault_config.spent = new_vault_spent;

        env.storage()
            .persistent()
            .set(&delegate_key, &delegate_config);
        env.storage().persistent().set(&vault_key, &vault_config);
    }

    /// Deactivate vault (gracefully handles depletion)
    pub fn deactivate_vault(env: Env, vault_id: Address) {
        let vault_key = DataKey::FamilyVault(vault_id.clone());
        let mut vault_config: FamilyVaultConfig = env
            .storage()
            .persistent()
            .get(&vault_key)
            .expect("vault not found");

        vault_config.owner.require_auth();
        vault_config.is_active = false;

        env.storage().persistent().set(&vault_key, &vault_config);
    }

    // =========================================================================
    // Issue #122: Merchant-Triggered "Vacation Mode" Pause
    // =========================================================================

    /// Activate vacation mode - pauses all subscription billing for this merchant
    pub fn activate_vacation_mode(env: Env, merchant: Address) {
        merchant.require_auth();

        let vacation_key = DataKey::MerchantVacationMode(merchant.clone());
        
        // Check if already active
        if env.storage().persistent().has(&vacation_key) {
            let existing: VacationModeStatus = env.storage().persistent().get(&vacation_key).unwrap();
            if existing.is_active {
                panic!("vacation mode already active");
            }
        }

        let now = env.ledger().timestamp();
        let vacation_status = VacationModeStatus {
            is_active: true,
            activated_at: now,
            deactivated_at: 0,
            pause_duration: 0,
        };

        env.storage().persistent().set(&vacation_key, &vacation_status);

        VacationModeActivated {
            merchant,
            activated_at: now,
        }
        .publish(&env);
    }

    /// Deactivate vacation mode - resumes subscription billing
    pub fn deactivate_vacation_mode(env: Env, merchant: Address) {
        merchant.require_auth();

        let vacation_key = DataKey::MerchantVacationMode(merchant.clone());
        let mut vacation_status: VacationModeStatus = env
            .storage()
            .persistent()
            .get(&vacation_key)
            .expect("vacation mode not active");

        if !vacation_status.is_active {
            panic!("vacation mode not active");
        }

        let now = env.ledger().timestamp();
        let pause_duration = now - vacation_status.activated_at;
        
        vacation_status.is_active = false;
        vacation_status.deactivated_at = now;
        vacation_status.pause_duration += pause_duration;

        env.storage().persistent().set(&vacation_key, &vacation_status);

        // Adjust all active subscriptions to account for paused time
        adjust_subscriptions_for_vacation(&env, &merchant, pause_duration);

        VacationModeDeactivated {
            merchant,
            deactivated_at: now,
            total_pause_duration: vacation_status.pause_duration,
        }
        .publish(&env);
    }

    /// Check if merchant is in vacation mode
    pub fn is_vacation_mode_active(env: Env, merchant: Address) -> bool {
        let vacation_key = DataKey::MerchantVacationMode(merchant);
        if let Some(status) = env.storage().persistent().get(&vacation_key) {
            let status: VacationModeStatus = status;
            status.is_active
        } else {
            false
        }
    }

    // =========================================================================
    // Issue #123: Affiliate Referral Fee Routing
    // =========================================================================

    /// Configure affiliate program for a merchant
    pub fn configure_affiliate_program(
        env: Env,
        merchant: Address,
        commission_bps: u32,
        min_payout: i128,
    ) {
        merchant.require_auth();

        if commission_bps > 10000 {
            panic!("commission cannot exceed 100%");
        }
        if min_payout < 0 {
            panic!("min payout must be non-negative");
        }

        let affiliate_config = AffiliateConfig {
            commission_bps,
            is_enabled: true,
            min_payout,
            total_paid: 0,
        };

        env.storage()
            .persistent()
            .set(&DataKey::AffiliateConfig(merchant.clone()), &affiliate_config);

        AffiliateConfigured {
            merchant,
            commission_bps,
            configured_at: env.ledger().timestamp(),
        }
        .publish(&env);
    }

    /// Record an affiliate referral when a user subscribes
    pub fn record_affiliate_referral(
        env: Env,
        merchant: Address,
        affiliate: Address,
        referred_user: Address,
        subscription_amount: i128,
    ) {
        // Prevent self-referral
        if affiliate == referred_user {
            panic!("self-referral not allowed");
        }

        // Verify affiliate program is enabled
        let config_key = DataKey::AffiliateConfig(merchant.clone());
        let affiliate_config: AffiliateConfig = env
            .storage()
            .persistent()
            .get(&config_key)
            .expect("affiliate program not configured");

        if !affiliate_config.is_enabled {
            panic!("affiliate program not enabled");
        }

        // Calculate commission
        let commission = (subscription_amount * affiliate_config.commission_bps as i128) / 10000;

        // Update affiliate tracking
        let referral_key = DataKey::AffiliateReferral(merchant.clone(), affiliate.clone());
        let mut referral_info: AffiliateReferralInfo = env
            .storage()
            .persistent()
            .get(&referral_key)
            .unwrap_or(AffiliateReferralInfo {
                affiliate: affiliate.clone(),
                referral_count: 0,
                total_earned: 0,
                total_claimed: 0,
                last_payout_at: 0,
            });

        referral_info.referral_count += 1;
        referral_info.total_earned += commission;

        env.storage().persistent().set(&referral_key, &referral_info);

        AffiliateReferralRecorded {
            merchant,
            affiliate,
            referred_user,
            commission_amount: commission,
            recorded_at: env.ledger().timestamp(),
        }
        .publish(&env);
    }

    /// Affiliate claims their earned commissions
    pub fn claim_affiliate_payout(env: Env, merchant: Address, affiliate: Address) {
        affiliate.require_auth();

        // Get affiliate config
        let config_key = DataKey::AffiliateConfig(merchant.clone());
        let mut affiliate_config: AffiliateConfig = env
            .storage()
            .persistent()
            .get(&config_key)
            .expect("affiliate program not configured");

        // Get affiliate referral info
        let referral_key = DataKey::AffiliateReferral(merchant.clone(), affiliate.clone());
        let mut referral_info: AffiliateReferralInfo = env
            .storage()
            .persistent()
            .get(&referral_key)
            .expect("no referral info found");

        // Calculate pending payout
        let pending = referral_info.total_earned - referral_info.total_claimed;
        
        if pending < affiliate_config.min_payout {
            panic!("below minimum payout threshold");
        }
        if pending <= 0 {
            panic!("no pending payout");
        }

        // Transfer payout from merchant to affiliate
        // Note: In production, this would need merchant's token approval
        // For now, we assume the merchant has pre-funded a payout pool
        let payout_token = DataKey::AcceptedToken(merchant.clone());
        if let Some(token) = env.storage().persistent().get(&payout_token) {
            let token_client = TokenClient::new(&env, &token);
            token_client.transfer(&merchant, &affiliate, &pending);
        }

        // Update tracking
        referral_info.total_claimed += pending;
        referral_info.last_payout_at = env.ledger().timestamp();
        affiliate_config.total_paid += pending;

        env.storage().persistent().set(&referral_key, &referral_info);
        env.storage()
            .persistent()
            .set(&config_key, &affiliate_config);

        AffiliatePayoutClaimed {
            merchant,
            affiliate,
            payout_amount: pending,
            claimed_at: env.ledger().timestamp(),
        }
        .publish(&env);
    }

    /// Get affiliate referral info
    pub fn get_affiliate_info(env: Env, merchant: Address, affiliate: Address) -> AffiliateReferralInfo {
        let referral_key = DataKey::AffiliateReferral(merchant, affiliate);
        env.storage()
            .persistent()
            .get(&referral_key)
            .unwrap_or(AffiliateReferralInfo {
                affiliate: Address::generate(&env),
                referral_count: 0,
                total_earned: 0,
                total_claimed: 0,
                last_payout_at: 0,
            })
    }

    // =========================================================================
    // Issue #127: get_active_subscriptions Read-Only Query
    // =========================================================================

    /// Read-only query to get all active subscriptions for a subscriber
    /// Returns comprehensive data ready for UI rendering
    pub fn get_active_subscriptions(env: Env, subscriber: Address) -> soroban_sdk::Vec<Subscription> {
        let index_key = DataKey::SubscriberIndex(subscriber.clone());
        let creators: soroban_sdk::Vec<Address> = env
            .storage()
            .persistent()
            .get(&index_key)
            .unwrap_or(soroban_sdk::Vec::new(&env));

        let mut active_subs = soroban_sdk::vec![&env];
        for i in 0..creators.len() {
            let creator = creators.get(i).unwrap();
            let key = subscription_key(&subscriber, &creator);
            if subscription_exists(&env, &key) {
                active_subs.push_back(get_subscription(&env, &key));
            }
        }
        active_subs
    }

    // =========================================================================
    // Issue #129: Cursor-Based Pagination for User Queries
    // =========================================================================

    /// Returns a paginated slice of a subscriber's active subscriptions.
    /// `cursor` is the 0-based index to start from; `limit` caps the page size.
    /// Returns `next_cursor` = cursor + items_returned and `has_more` flag.
    /// Out-of-bounds cursors return an empty page without panicking.
    pub fn get_subscriptions_paginated(
        env: Env,
        subscriber: Address,
        cursor: u32,
        limit: u32,
    ) -> SubscriptionPage {
        if limit == 0 {
            return SubscriptionPage {
                items: soroban_sdk::Vec::new(&env),
                next_cursor: cursor,
                has_more: false,
            };
        }

        let index_key = DataKey::SubscriberIndex(subscriber.clone());
        let creators: soroban_sdk::Vec<Address> = env
            .storage()
            .persistent()
            .get(&index_key)
            .unwrap_or(soroban_sdk::Vec::new(&env));

        let total = creators.len();
        if cursor >= total || total == 0 {
            return SubscriptionPage {
                items: soroban_sdk::Vec::new(&env),
                next_cursor: cursor,
                has_more: false,
            };
        }

        let mut items = soroban_sdk::Vec::new(&env);
        let end = (cursor + limit).min(total);
        for i in cursor..end {
            let creator = creators.get(i).unwrap();
            let key = subscription_key(&subscriber, &creator);
            if subscription_exists(&env, &key) {
                items.push_back(get_subscription(&env, &key));
            }
        }

        let next_cursor = end;
        let has_more = next_cursor < total;
        SubscriptionPage { items, next_cursor, has_more }
    }

    // =========================================================================
    // Issue #131: check_allowance_health Pre-flight Check
    // =========================================================================

    /// Read-only pre-flight check for merchants.
    /// Returns Healthy, InsufficientFunds, or AllowanceRevoked without
    /// leaking the subscriber's exact wallet balance.
    pub fn check_allowance_health(
        env: Env,
        subscriber: Address,
        creator: Address,
        token: Address,
    ) -> AllowanceHealth {
        let key = subscription_key(&subscriber, &creator);
        if !subscription_exists(&env, &key) {
            return AllowanceHealth::AllowanceRevoked;
        }

        let sub = get_subscription(&env, &key);
        let token_client = TokenClient::new(&env, &token);

        // Check if the contract still holds an allowance from the subscriber.
        // In Soroban the subscriber pre-funds the contract; a zero balance means
        // the allowance has effectively been revoked (funds exhausted / not topped up).
        let contract_balance = token_client.balance(&env.current_contract_address());
        if contract_balance <= 0 {
            return AllowanceHealth::AllowanceRevoked;
        }

        // Determine the next pull amount (one billing cycle worth of charges).
        let now = env.ledger().timestamp();
        let trial_end = sub.start_time.saturating_add(sub.tier.trial_duration);
        let charge_start = if sub.last_collected > trial_end {
            sub.last_collected
        } else {
            trial_end
        };

        // Estimate upcoming charge as one second of rate (conservative lower bound).
        let upcoming_charge = sub.tier.rate_per_second;

        if upcoming_charge <= 0 || now <= charge_start {
            return AllowanceHealth::Healthy;
        }

        // Compare subscriber's share of the contract balance against upcoming charge.
        // We use the subscription's stored balance (in nano units) as the proxy —
        // this avoids leaking the subscriber's total wallet balance.
        let balance_tokens = sub.balance / PRECISION_MULTIPLIER;
        if balance_tokens >= upcoming_charge {
            AllowanceHealth::Healthy
        } else {
            AllowanceHealth::InsufficientFunds
        }
    }

    // =========================================================================
    // Issue #134: Data-Pruning for Canceled Subscriptions
    // =========================================================================

    /// Prune a subscription that has been canceled for more than 90 days.
    /// Deletes the billing history and leaves a lightweight tombstone hash.
    /// Emits StaleDataPruned so indexers know the data is gone.
    /// Active or recently canceled subscriptions are never pruned.
    pub fn prune_stale_data(env: Env, subscriber: Address, creator: Address) {
        const NINETY_DAYS: u64 = 90 * 24 * 60 * 60;

        let canceled_key = DataKey::CanceledRecord(subscriber.clone(), creator.clone());
        let record: CanceledRecord = env
            .storage()
            .persistent()
            .get(&canceled_key)
            .expect("no canceled record found");

        let now = env.ledger().timestamp();
        if now < record.canceled_at.saturating_add(NINETY_DAYS) {
            panic!("subscription canceled less than 90 days ago");
        }

        // Remove the canceled record (billing history).
        env.storage().persistent().remove(&canceled_key);

        // Build a deterministic tombstone: sha256-like hash via XDR bytes of the key.
        // In Soroban we use the env crypto primitives — here we encode the addresses
        // as a fixed-length byte sequence and hash it.
        let mut raw = soroban_sdk::Bytes::new(&env);
        raw.extend_from_array(&[0u8; 32]); // placeholder for subscriber bytes
        raw.extend_from_array(&[0u8; 32]); // placeholder for creator bytes
        let tombstone = env.crypto().sha256(&raw);
        let tombstone_bytes = soroban_sdk::Bytes::from_array(&env, &tombstone.to_array());

        env.storage().persistent().set(
            &DataKey::Tombstone(subscriber.clone(), creator.clone()),
            &tombstone_bytes,
        );

        StaleDataPruned {
            subscriber,
            creator,
            tombstone: tombstone_bytes,
            pruned_at: now,
        }
        .publish(&env);
    }

    /// Returns the tombstone hash for a pruned subscription, if it exists.
    pub fn get_tombstone(env: Env, subscriber: Address, creator: Address) -> Option<soroban_sdk::Bytes> {
        env.storage()
            .persistent()
            .get(&DataKey::Tombstone(subscriber, creator))
    }
}

/// Helper function to adjust all active subscriptions for a merchant after vacation mode
/// This extends the subscription duration by the pause duration to preserve paid-for time
fn adjust_subscriptions_for_vacation(env: &Env, merchant: &Address, pause_duration: u64) {
    // In a production system, you would iterate through all active subscriptions
    // For now, this is a placeholder that demonstrates the concept
    // The actual implementation would:
    // 1. Find all subscriptions for this merchant
    // 2. For each active subscription, adjust the last_collected time forward by pause_duration
    // 3. This effectively extends their subscription without additional charge
    
    // Example logic (would need subscription index to implement fully):
    // for each subscription where creators.contains(merchant):
    //     sub.last_collected += pause_duration;
    //     set_subscription(env, &key, &sub);
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

pub(crate) fn register_creator_support(env: &Env, creator: &Address, beneficiary: &Address) {
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

pub(crate) fn unregister_creator_support(env: &Env, creator: &Address, beneficiary: &Address) {
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

    // --- Issue #128: also update merchant metrics revenue ---
    let metrics_key = DataKey::MerchantMetrics(creator.clone());
    if let Some(mut m) = env.storage().persistent().get::<MerchantMetrics>(&metrics_key) {
        m.total_revenue = m.total_revenue.saturating_add(amount);
        m.avg_revenue_per_subscriber = if m.active_subscribers > 0 {
            m.total_revenue / m.active_subscribers as i128
        } else {
            0
        };
        m.last_updated = env.ledger().timestamp();
        env.storage().persistent().set(&metrics_key, &m);
    }
}

fn update_top_fans(env: &Env, creator: &Address, fan: &Address, new_amount: i128) {
    let key = DataKey::TopFans(creator.clone());
    let mut top_fans: soroban_sdk::Vec<TopFan> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(soroban_sdk::Vec::new(env));

    let mut existing_idx: Option<u32> = None;
    for i in 0..top_fans.len() {
        if top_fans.get(i).unwrap().fan == *fan {
            existing_idx = Some(i);
            break;
        }
    }

    if let Some(idx) = existing_idx {
        // Update existing entry
        top_fans.set(idx, TopFan {
            fan: fan.clone(),
            amount: new_amount,
        });
        // Bubble up if needed
        let mut curr = idx;
        while curr > 0 {
            let prev = curr - 1;
            if top_fans.get(prev).unwrap().amount < top_fans.get(curr).unwrap().amount {
                let p_val = top_fans.get(prev).unwrap();
                let c_val = top_fans.get(curr).unwrap();
                top_fans.set(prev, c_val);
                top_fans.set(curr, p_val);
                curr = prev;
            } else {
                break;
            }
        }
    } else {
        // New fan
        if top_fans.len() < 50 {
            top_fans.push_back(TopFan {
                fan: fan.clone(),
                amount: new_amount,
            });
            // Bubble up
            let mut curr = top_fans.len() - 1;
            while curr > 0 {
                let prev = curr - 1;
                if top_fans.get(prev).unwrap().amount < top_fans.get(curr).unwrap().amount {
                    let p_val = top_fans.get(prev).unwrap();
                    let c_val = top_fans.get(curr).unwrap();
                    top_fans.set(prev, c_val);
                    top_fans.set(curr, p_val);
                    curr = prev;
                } else {
                    break;
                }
            }
        } else {
            // List full, check if we beat the last one (index 49)
            let last_idx = 49;
            if new_amount > top_fans.get(last_idx).unwrap().amount {
                top_fans.set(last_idx, TopFan {
                    fan: fan.clone(),
                    amount: new_amount,
                });
                // Bubble up
                let mut curr = last_idx;
                while curr > 0 {
                    let prev = curr - 1;
                    if top_fans.get(prev).unwrap().amount < top_fans.get(curr).unwrap().amount {
                        let p_val = top_fans.get(prev).unwrap();
                        let c_val = top_fans.get(curr).unwrap();
                        top_fans.set(prev, c_val);
                        top_fans.set(curr, p_val);
                        curr = prev;
                    } else {
                        break;
                    }
                }
            }
        }
    }

    env.storage().persistent().set(&key, &top_fans);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_BUMP_AMOUNT);
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

    // Update the top 50 leaderboard
    update_top_fans(env, creator, fan, next);

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
    if sub.flags & FLAGS_FREE_TO_PAID == 0 && sub.tier.rate_per_second > 0 && now > trial_end {
        FreeToPaidTierActivated {
            subscriber: beneficiary.clone(),
            creator: stream_id.clone(),
            rate_per_second: sub.tier.rate_per_second,
            activated_at: now,
        }
        .publish(env);
        // Issue #133: emit TrialAutoConverted analytics event
        TrialAutoConverted {
            subscriber: beneficiary.clone(),
            merchant: stream_id.clone(),
            is_first_payment: true,
            converted_at: now,
        }
        .publish(env);
        sub.flags |= FLAGS_FREE_TO_PAID;
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

    // Streak Reset Logic: If stream was interrupted (funds exhausted beyond grace period), reset streak.
    if sub.last_funds_exhausted > 0 {
        let grace_period_end = sub.last_funds_exhausted.saturating_add(GRACE_PERIOD);
        if now > grace_period_end {
            sub.streak_start_date = now; // Streak interrupted
            sub.last_funds_exhausted = 0; // Reset exhaustion state for the new streak
            set_subscription(env, &key, &sub);
            return 0;
        }
    }

    let amount_to_collect =
        calculate_discounted_charge(sub.streak_start_date, charge_start, now, sub.tier.rate_per_second);

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

    let now = env.ledger().timestamp();
    if sub.last_funds_exhausted > 0 {
        let grace_period_end = sub.last_funds_exhausted.saturating_add(GRACE_PERIOD);
        if now > grace_period_end {
            sub.streak_start_date = now; // Streak interrupted
        }
    }

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
        unregister_creator_support(env, &creator, beneficiary);
    }
    let billing_key = DataKey::BillingCycle(beneficiary.clone(), stream_id.clone());
    env.storage().persistent().remove(&billing_key);
    env.storage()
        .persistent()
        .remove(&DataKey::PendingMerchantPull(beneficiary.clone(), stream_id.clone()));
    env.storage().persistent().remove(&key);
    env.storage().temporary().remove(&key);

    // Issue #134: store a lightweight canceled record so prune_stale_data can verify age
    let canceled_record = CanceledRecord {
        token: sub.token.clone(),
        canceled_at: now,
    };
    env.storage().persistent().set(
        &DataKey::CanceledRecord(beneficiary.clone(), stream_id.clone()),
        &canceled_record,
    );

    // Issue #129: remove creator from subscriber's index
    let index_key = DataKey::SubscriberIndex(beneficiary.clone());
    if let Some(mut index) = env.storage().persistent().get::<soroban_sdk::Vec<Address>>(&index_key) {
        let mut remove_idx: Option<u32> = None;
        for i in 0..index.len() {
            if index.get(i).unwrap() == *stream_id {
                remove_idx = Some(i);
                break;
            }
        }
        if let Some(idx) = remove_idx {
            index.remove(idx);
            env.storage().persistent().set(&index_key, &index);
        }
    }

    record_protocol_cancellation(env);
    Unsubscribed {
        subscriber: beneficiary.clone(),
        creator: stream_id.clone(),
    }
    .publish(env);
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
        streak_start_date: now,
        last_funds_exhausted: 0,
        flags: 0,
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

    // --- Issue #125: Snapshot the active ToS at subscribe-time ---
    if let Some(tos_anchor) = env
        .storage()
        .persistent()
        .get::<MerchantToSAnchor>(&DataKey::MerchantToS(stream_id.clone()))
    {
        let now_ts = env.ledger().timestamp();
        let snapshot = ToSSnapshot {
            ipfs_hash: tos_anchor.ipfs_hash.clone(),
            version: tos_anchor.version,
            agreed_at: now_ts,
        };
        env.storage().persistent().set(
            &DataKey::SubscriptionToSSnapshot(beneficiary.clone(), stream_id.clone()),
            &snapshot,
        );
        ToSAgreed {
            subscriber: beneficiary.clone(),
            merchant: stream_id.clone(),
            tos_version: tos_anchor.version,
            ipfs_hash: tos_anchor.ipfs_hash,
            agreed_at: now_ts,
        }
        .publish(env);
    }

    // --- Issue #128 + Issue #126: update merchant metrics & emit standardized event ---
    {
        let metrics_key = DataKey::MerchantMetrics(stream_id.clone());
        let mut m: MerchantMetrics = env
            .storage()
            .persistent()
            .get(&metrics_key)
            .unwrap_or(MerchantMetrics {
                total_subscribers: 0,
                active_subscribers: 0,
                dunning_subscribers: 0,
                total_revenue: 0,
                avg_revenue_per_subscriber: 0,
                last_updated: 0,
            });
        m.total_subscribers = m.total_subscribers.saturating_add(1);
        m.active_subscribers = m.active_subscribers.saturating_add(1);
        m.last_updated = env.ledger().timestamp();
        env.storage().persistent().set(&metrics_key, &m);

        // Issue #126: standardized SubscriptionCreated event for subgraph parity
        SubscriptionCreated {
            subscriber: beneficiary.clone(),
            merchant: stream_id.clone(),
            plan_id: 0, // plan_id not tracked in streaming path; use 0 as default
            token: token.clone(),
            rate_per_second: rate,
            created_at: env.ledger().timestamp(),
        }
        .publish(env);
    }

    Subscribed {
        subscriber: beneficiary.clone(),
        creator: stream_id.clone(),
        rate_per_second: rate,
    }
    .publish(env);

    // Issue #129: maintain per-subscriber index for pagination
    let index_key = DataKey::SubscriberIndex(beneficiary.clone());
    let mut index: soroban_sdk::Vec<Address> = env
        .storage()
        .persistent()
        .get(&index_key)
        .unwrap_or(soroban_sdk::Vec::new(env));
    // Only add if not already present (idempotent)
    let mut found = false;
    for i in 0..index.len() {
        if index.get(i).unwrap() == *stream_id {
            found = true;
            break;
        }
    }
    if !found {
        index.push_back(stream_id.clone());
        env.storage().persistent().set(&index_key, &index);
        env.storage()
            .persistent()
            .extend_ttl(&index_key, TTL_THRESHOLD, TTL_BUMP_AMOUNT);
    }
}

fn is_creator_paused(env: &Env, creator: &Address) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::ChannelPaused(creator.clone()))
        .unwrap_or(false)
}

fn require_contract_admin(env: &Env, admin: &Address) {
    let stored_admin: Address = env
        .storage()
        .persistent()
        .get(&DataKey::ContractAdmin)
        .expect("not initialized");
    if admin != &stored_admin {
        panic!("admin only");
    }
}

fn require_protocol_soft_pause_inactive(env: &Env) {
    if read_velocity_circuit_breaker_state(env).soft_pause_active {
        panic!("protocol soft paused");
    }
}

fn default_hourly_cancel_buckets(env: &Env) -> soroban_sdk::Vec<HourlyCancelBucket> {
    let mut buckets = soroban_sdk::Vec::new(env);
    for _ in 0..CANCEL_VELOCITY_HOURLY_BUCKETS {
        buckets.push_back(HourlyCancelBucket {
            hour_epoch: 0,
            count: 0,
        });
    }
    buckets
}

fn default_daily_cancel_buckets(env: &Env) -> soroban_sdk::Vec<DailyCancelBucket> {
    let mut buckets = soroban_sdk::Vec::new(env);
    for _ in 0..CANCEL_VELOCITY_DAILY_BUCKETS {
        buckets.push_back(DailyCancelBucket {
            day_epoch: 0,
            count: 0,
        });
    }
    buckets
}

fn default_velocity_circuit_breaker_state() -> VelocityCircuitBreakerState {
    VelocityCircuitBreakerState {
        active: false,
        soft_pause_active: false,
        triggered_at: 0,
        last_updated: 0,
        last_velocity: 0,
        last_threshold: CANCEL_VELOCITY_MIN_TRIGGER,
    }
}

fn read_hourly_cancel_buckets(env: &Env) -> soroban_sdk::Vec<HourlyCancelBucket> {
    env.storage()
        .persistent()
        .get(&DataKey::CancelVelocityHourlyBuckets)
        .unwrap_or(default_hourly_cancel_buckets(env))
}

fn write_hourly_cancel_buckets(env: &Env, buckets: &soroban_sdk::Vec<HourlyCancelBucket>) {
    env.storage()
        .persistent()
        .set(&DataKey::CancelVelocityHourlyBuckets, buckets);
    env.storage().persistent().extend_ttl(
        &DataKey::CancelVelocityHourlyBuckets,
        TTL_THRESHOLD,
        TTL_BUMP_AMOUNT,
    );
}

fn read_daily_cancel_buckets(env: &Env) -> soroban_sdk::Vec<DailyCancelBucket> {
    env.storage()
        .persistent()
        .get(&DataKey::CancelVelocityDailyBuckets)
        .unwrap_or(default_daily_cancel_buckets(env))
}

fn write_daily_cancel_buckets(env: &Env, buckets: &soroban_sdk::Vec<DailyCancelBucket>) {
    env.storage()
        .persistent()
        .set(&DataKey::CancelVelocityDailyBuckets, buckets);
    env.storage().persistent().extend_ttl(
        &DataKey::CancelVelocityDailyBuckets,
        TTL_THRESHOLD,
        TTL_BUMP_AMOUNT,
    );
}

fn read_velocity_circuit_breaker_state(env: &Env) -> VelocityCircuitBreakerState {
    env.storage()
        .persistent()
        .get(&DataKey::CancelVelocityBreakerState)
        .unwrap_or(default_velocity_circuit_breaker_state())
}

fn write_velocity_circuit_breaker_state(env: &Env, state: &VelocityCircuitBreakerState) {
    env.storage()
        .persistent()
        .set(&DataKey::CancelVelocityBreakerState, state);
    env.storage().persistent().extend_ttl(
        &DataKey::CancelVelocityBreakerState,
        TTL_THRESHOLD,
        TTL_BUMP_AMOUNT,
    );
}

fn prune_hourly_cancel_buckets(now: u64, buckets: &mut soroban_sdk::Vec<HourlyCancelBucket>) {
    let current_hour = now / HOUR_IN_SECONDS;
    for i in 0..buckets.len() {
        let mut bucket = buckets.get(i).unwrap();
        if bucket.count > 0
            && current_hour.saturating_sub(bucket.hour_epoch)
                >= CANCEL_VELOCITY_HOURLY_BUCKETS as u64
        {
            bucket.hour_epoch = 0;
            bucket.count = 0;
            buckets.set(i, bucket);
        }
    }
}

fn prune_daily_cancel_buckets(now: u64, buckets: &mut soroban_sdk::Vec<DailyCancelBucket>) {
    let current_day = now / DAY_IN_SECONDS;
    for i in 0..buckets.len() {
        let mut bucket = buckets.get(i).unwrap();
        if bucket.count > 0
            && current_day.saturating_sub(bucket.day_epoch)
                >= CANCEL_VELOCITY_DAILY_BUCKETS as u64
        {
            bucket.day_epoch = 0;
            bucket.count = 0;
            buckets.set(i, bucket);
        }
    }
}

fn sum_hourly_cancel_buckets(now: u64, buckets: &soroban_sdk::Vec<HourlyCancelBucket>) -> u32 {
    let current_hour = now / HOUR_IN_SECONDS;
    let mut total = 0u32;
    for i in 0..buckets.len() {
        let bucket = buckets.get(i).unwrap();
        if bucket.count > 0
            && current_hour.saturating_sub(bucket.hour_epoch)
                < CANCEL_VELOCITY_HOURLY_BUCKETS as u64
        {
            total = total.saturating_add(bucket.count);
        }
    }
    total
}

fn sum_daily_cancel_buckets(now: u64, buckets: &soroban_sdk::Vec<DailyCancelBucket>) -> u32 {
    let current_day = now / DAY_IN_SECONDS;
    let mut total = 0u32;
    for i in 0..buckets.len() {
        let bucket = buckets.get(i).unwrap();
        if bucket.count > 0
            && current_day.saturating_sub(bucket.day_epoch)
                < CANCEL_VELOCITY_DAILY_BUCKETS as u64
        {
            total = total.saturating_add(bucket.count);
        }
    }
    total
}

fn calculate_cancel_velocity_threshold(trailing_30d_cancellations: u32) -> (u32, u32) {
    let daily_average = if trailing_30d_cancellations == 0 {
        0
    } else {
        (trailing_30d_cancellations + (CANCEL_VELOCITY_DAILY_BUCKETS - 1))
            / CANCEL_VELOCITY_DAILY_BUCKETS
    };
    let baseline = daily_average.max(1);
    let threshold = baseline
        .saturating_mul(CANCEL_VELOCITY_MULTIPLIER)
        .max(CANCEL_VELOCITY_MIN_TRIGGER);
    (daily_average, threshold)
}

fn sync_cancel_velocity_metrics(env: &Env) -> CancelVelocityMetrics {
    let now = env.ledger().timestamp();
    let mut hourly_buckets = read_hourly_cancel_buckets(env);
    let mut daily_buckets = read_daily_cancel_buckets(env);

    prune_hourly_cancel_buckets(now, &mut hourly_buckets);
    prune_daily_cancel_buckets(now, &mut daily_buckets);

    let rolling_24h_cancellations = sum_hourly_cancel_buckets(now, &hourly_buckets);
    let trailing_30d_cancellations = sum_daily_cancel_buckets(now, &daily_buckets);
    let (daily_average_30d, anomaly_threshold) =
        calculate_cancel_velocity_threshold(trailing_30d_cancellations);

    let mut state = read_velocity_circuit_breaker_state(env);
    state.last_updated = now;
    state.last_velocity = rolling_24h_cancellations;
    state.last_threshold = anomaly_threshold;

    write_hourly_cancel_buckets(env, &hourly_buckets);
    write_daily_cancel_buckets(env, &daily_buckets);
    write_velocity_circuit_breaker_state(env, &state);

    CancelVelocityMetrics {
        rolling_24h_cancellations,
        trailing_30d_cancellations,
        daily_average_30d,
        anomaly_threshold,
        circuit_breaker_active: state.active,
        soft_pause_active: state.soft_pause_active,
        triggered_at: state.triggered_at,
        hourly_bucket_count: hourly_buckets.len(),
        daily_bucket_count: daily_buckets.len(),
    }
}

fn record_protocol_cancellation(env: &Env) -> CancelVelocityMetrics {
    let now = env.ledger().timestamp();
    let current_hour = now / HOUR_IN_SECONDS;
    let current_day = now / DAY_IN_SECONDS;

    let mut hourly_buckets = read_hourly_cancel_buckets(env);
    let mut daily_buckets = read_daily_cancel_buckets(env);
    prune_hourly_cancel_buckets(now, &mut hourly_buckets);
    prune_daily_cancel_buckets(now, &mut daily_buckets);

    let hourly_idx = (current_hour % CANCEL_VELOCITY_HOURLY_BUCKETS as u64) as u32;
    let daily_idx = (current_day % CANCEL_VELOCITY_DAILY_BUCKETS as u64) as u32;

    let mut hour_bucket = hourly_buckets.get(hourly_idx).unwrap();
    if hour_bucket.hour_epoch != current_hour {
        hour_bucket.hour_epoch = current_hour;
        hour_bucket.count = 0;
    }
    hour_bucket.count = hour_bucket.count.saturating_add(1);
    hourly_buckets.set(hourly_idx, hour_bucket);

    let mut day_bucket = daily_buckets.get(daily_idx).unwrap();
    if day_bucket.day_epoch != current_day {
        day_bucket.day_epoch = current_day;
        day_bucket.count = 0;
    }
    day_bucket.count = day_bucket.count.saturating_add(1);
    daily_buckets.set(daily_idx, day_bucket);

    let rolling_24h_cancellations = sum_hourly_cancel_buckets(now, &hourly_buckets);
    let trailing_30d_cancellations = sum_daily_cancel_buckets(now, &daily_buckets);
    let (daily_average_30d, anomaly_threshold) =
        calculate_cancel_velocity_threshold(trailing_30d_cancellations);

    let mut state = read_velocity_circuit_breaker_state(env);
    state.last_updated = now;
    state.last_velocity = rolling_24h_cancellations;
    state.last_threshold = anomaly_threshold;

    if !state.active && rolling_24h_cancellations > anomaly_threshold {
        state.active = true;
        state.soft_pause_active = true;
        state.triggered_at = now;

        VelocityAnomalyDetected {
            current_velocity: rolling_24h_cancellations,
            threshold: anomaly_threshold,
            timestamp: now,
        }
        .publish(env);
    }

    write_hourly_cancel_buckets(env, &hourly_buckets);
    write_daily_cancel_buckets(env, &daily_buckets);
    write_velocity_circuit_breaker_state(env, &state);

    CancelVelocityMetrics {
        rolling_24h_cancellations,
        trailing_30d_cancellations,
        daily_average_30d,
        anomaly_threshold,
        circuit_breaker_active: state.active,
        soft_pause_active: state.soft_pause_active,
        triggered_at: state.triggered_at,
        hourly_bucket_count: hourly_buckets.len(),
        daily_bucket_count: daily_buckets.len(),
    }
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

mod test_enhanced_subscriptions;
#[cfg(test)]
mod test_merchant_registry;
#[cfg(test)]
mod test_formal_verification;
#[cfg(test)]
mod test_reentrancy_guard;
#[cfg(test)]
mod test_timelock_governance;
#[cfg(test)]
mod test_velocity_circuit_breaker;
#[cfg(test)]
mod test_analytics_events;
#[cfg(test)]
mod test_stress_500_pulls;
#[cfg(test)]
mod test_clock_drift_fuzz;
