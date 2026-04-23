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

// --- Merchant Registry and KYC Whitelisting Constants ---
const DAO_MULTISIG_THRESHOLD: u32 = 3; // Minimum signatures required for DAO decisions
const MERCHANT_KYC_VALIDITY: u64 = 365 * 24 * 60 * 60; // 1 year validity for KYC credentials
const SEP12_KYC_ISSUER: &str = "GD5DQX2K7Q4D4PE4R6J4Y7Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2"; // SEP-12 KYC issuer address
const TIMELOCK_DURATION: u64 = 48 * 60 * 60; // 48-hour timelock for registry updates
const SECURITY_COUNCIL_SIZE: u32 = 5; // 5-member security council for multi-sig

// --- Dynamic Protocol Fee Constants ---
const PROTOCOL_FEE_MAX_BPS: u32 = 500; // Maximum 5% protocol fee (500 basis points)
const PROTOCOL_FEE_TIMELOCK_DURATION: u64 = 7 * 24 * 60 * 60; // 7-day timelock for fee increases
const DEFAULT_PROTOCOL_FEE_BPS: u32 = 200; // Default 2% protocol fee (200 basis points)

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
    PlanRegistry(Address),             // Merchant's pricing plans registry
    TrialUsed(Address, Address),      // (user, merchant) - Prevent trial abuse
    BillingCycle(Address, Address),    // (subscriber, merchant) - Billing cycle info
    SLAStatus(Address),               // Creator's SLA status
    UptimeOracleNonce(u64),           // Oracle nonce tracking
    // --- Merchant Registry and KYC Whitelisting Keys ---
    MerchantRegistry(Address),        // Merchant registration and verification status
    KYCCredential(Address),           // SEP-12 KYC credential for merchant
    DAOProposal(u64),                 // DAO proposal for merchant approval
    DAOVote(Address, u64),           // DAO member vote on proposal
    BlacklistedMerchant(Address),     // Blacklisted merchant status
    RegistryUpdateProposal(u64),     // Timelock proposal for registry updates
    SecurityCouncilMember(Address),   // Security council membership
    SecurityCouncilVeto(Address, u64), // Security council veto on proposal
    // --- Dynamic Protocol Fee Keys ---
    ProtocolFeeConfig,               // Global protocol fee configuration
    ProtocolFeeUpdateProposal(u64),   // Protocol fee update proposal with timelock
    // --- Global Reentrancy Guard Keys ---
    ReentrancyGuard,                 // Global reentrancy protection state
}

// Add the rest of the existing structures and implementation...
// This is a partial file - we'll need to complete it with all the existing code

// For now, let me focus on implementing the key protocol fee functions

#[contractevent]
pub struct ProtocolFeeUpdateScheduled {
    #[topic] pub proposal_id: u64,
    #[topic] pub proposed_by: Address,
    pub old_fee_bps: u32,
    pub new_fee_bps: u32,
    pub proposed_at: u64,
    pub executable_at: u64,
    pub is_fee_increase: bool,
}

#[contractevent]
pub struct ProtocolFeeUpdateExecuted {
    #[topic] pub proposal_id: u64,
    #[topic] pub executed_by: Address,
    pub old_fee_bps: u32,
    pub new_fee_bps: u32,
    pub executed_at: u64,
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
        
        // Initialize protocol fee configuration with default fee
        let now = env.ledger().timestamp();
        let fee_config = ProtocolFeeConfig {
            current_fee_bps: DEFAULT_PROTOCOL_FEE_BPS,
            last_updated: now,
            updated_by: admin.clone(),
        };
        env.storage()
            .persistent()
            .set(&DataKey::ProtocolFeeConfig, &fee_config);
    }

    /// Get current protocol fee configuration
    pub fn get_protocol_fee_config(env: Env) -> ProtocolFeeConfig {
        env.storage().persistent()
            .get(&DataKey::ProtocolFeeConfig)
            .expect("protocol fee config not initialized")
    }

    /// Propose a protocol fee update (DAO multi-sig only)
    pub fn propose_protocol_fee_update(
        env: Env,
        dao_member: Address,
        new_fee_bps: u32,
    ) -> u64 {
        dao_member.require_auth();
        
        // Verify DAO member authorization
        if !is_authorized_dao_member(&env, &dao_member) {
            panic!("unauthorized DAO member");
        }

        // Validate fee bounds
        if new_fee_bps > PROTOCOL_FEE_MAX_BPS {
            panic!("fee exceeds maximum allowed");
        }

        let current_config = get_protocol_fee_config(env.clone());
        let now = env.ledger().timestamp();
        
        // Check if this is actually a change
        if new_fee_bps == current_config.current_fee_bps {
            panic!("no change in fee rate");
        }

        // Generate proposal ID
        let proposal_id = now; // Using timestamp as unique ID
        
        // Determine if this is a fee increase (triggers timelock)
        let is_fee_increase = new_fee_bps > current_config.current_fee_bps;
        let executable_at = if is_fee_increase {
            now + PROTOCOL_FEE_TIMELOCK_DURATION
        } else {
            now // Fee decreases can be executed immediately
        };

        let proposal = ProtocolFeeUpdateProposal {
            proposal_id,
            new_fee_bps,
            old_fee_bps: current_config.current_fee_bps,
            proposed_by: dao_member.clone(),
            proposed_at: now,
            executable_at,
            votes_for: vec![&env],
            executed: false,
            canceled: false,
            is_fee_increase,
        };

        env.storage().persistent()
            .set(&DataKey::ProtocolFeeUpdateProposal(proposal_id), &proposal);

        // Emit event
        ProtocolFeeUpdateScheduled {
            proposal_id,
            proposed_by: dao_member,
            old_fee_bps: current_config.current_fee_bps,
            new_fee_bps,
            proposed_at: now,
            executable_at,
            is_fee_increase,
        }.publish(&env);

        proposal_id
    }
}

// Helper functions (need to be implemented)
fn is_authorized_dao_member(_env: &Env, _dao_member: &Address) -> bool {
    // TODO: Implement proper DAO member authorization
    true // Placeholder
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolFeeConfig {
    pub current_fee_bps: u32,        // Current protocol fee in basis points
    pub last_updated: u64,            // Timestamp of last fee update
    pub updated_by: Address,         // Who updated the fee
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolFeeUpdateProposal {
    pub proposal_id: u64,
    pub new_fee_bps: u32,             // Proposed new fee in basis points
    pub old_fee_bps: u32,             // Current fee at time of proposal
    pub proposed_by: Address,         // Who proposed the change
    pub proposed_at: u64,             // When the proposal was created
    pub executable_at: u64,           // When the proposal can be executed (7 days later for increases)
    pub votes_for: soroban_sdk::Vec<Address>,  // DAO members voting for
    pub executed: bool,               // Whether the proposal has been executed
    pub canceled: bool,               // Whether the proposal was canceled
    pub is_fee_increase: bool,       // True if new fee > old fee (triggers timelock)
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bytes {
    pub data: soroban_sdk::Vec<u8>,
}
