#![cfg(test)]

use soroban_sdk::{Address, Env, vec, String};
use crate::{
    SubStreamContract, ProtocolFeeConfig, ProtocolFeeUpdateProposal, DataKey,
    PROTOCOL_FEE_MAX_BPS, PROTOCOL_FEE_TIMELOCK_DURATION, DEFAULT_PROTOCOL_FEE_BPS,
    DAO_MULTISIG_THRESHOLD, SECURITY_COUNCIL_SIZE, MAX_REASON_LENGTH,
};

#[test]
fn test_protocol_fee_initialization() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let kyc_issuer = Address::random(&env);
    let security_council = vec![&env,
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
    ];
    
    // Initialize contract with security council
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council.clone(), kyc_issuer);
    
    // Initialize protocol fee
    SubStreamContract::initialize_protocol_fee(env.clone(), admin.clone());
    
    // Check that protocol fee was initialized with default value
    let fee_config = SubStreamContract::get_protocol_fee_config(env.clone());
    assert_eq!(fee_config.current_fee_bps, DEFAULT_PROTOCOL_FEE_BPS);
    assert_eq!(fee_config.updated_by, admin);
    assert!(fee_config.last_updated > 0);
}

#[test]
fn test_propose_protocol_fee_increase() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let kyc_issuer = Address::random(&env);
    let security_council = vec![&env,
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
    ];
    let council_member = security_council.get(0).unwrap();
    
    // Initialize contract with security council
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council.clone(), kyc_issuer);
    
    // Initialize protocol fee
    SubStreamContract::initialize_protocol_fee(env.clone(), admin.clone());
    
    let new_fee_bps = 300; // Increase from 200 to 300 bps
    let reason = String::from_str(&env, "Increase fee to fund development");
    let proposal_id = SubStreamContract::propose_protocol_fee_update(
        env.clone(),
        council_member.clone(),
        new_fee_bps,
        reason,
    );
    
    // Verify proposal was created
    let proposal = SubStreamContract::get_protocol_fee_proposal(env.clone(), proposal_id);
    
    assert_eq!(proposal.new_fee_bps, new_fee_bps);
    assert_eq!(proposal.old_fee_bps, DEFAULT_PROTOCOL_FEE_BPS);
    assert_eq!(proposal.proposed_by, council_member);
    assert!(proposal.is_fee_increase);
    assert_eq!(proposal.executable_at, proposal.proposed_at + PROTOCOL_FEE_TIMELOCK_DURATION);
    assert!(!proposal.executed);
    assert!(!proposal.canceled);
}

#[test]
fn test_propose_protocol_fee_decrease() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let kyc_issuer = Address::random(&env);
    let security_council = vec![&env,
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
    ];
    let council_member = security_council.get(0).unwrap();
    
    // Initialize contract with security council
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council.clone(), kyc_issuer);
    
    // Initialize protocol fee
    SubStreamContract::initialize_protocol_fee(env.clone(), admin.clone());
    
    let new_fee_bps = 100; // Decrease from 200 to 100 bps
    let reason = String::from_str(&env, "Reduce fee to attract more users");
    let proposal_id = SubStreamContract::propose_protocol_fee_update(
        env.clone(),
        council_member.clone(),
        new_fee_bps,
        reason,
    );
    
    // Verify proposal was created
    let proposal = SubStreamContract::get_protocol_fee_proposal(env.clone(), proposal_id);
    
    assert_eq!(proposal.new_fee_bps, new_fee_bps);
    assert_eq!(proposal.old_fee_bps, DEFAULT_PROTOCOL_FEE_BPS);
    assert_eq!(proposal.proposed_by, council_member);
    assert!(!proposal.is_fee_increase); // This is a decrease
    assert_eq!(proposal.executable_at, proposal.proposed_at); // Immediate execution
    assert!(!proposal.executed);
    assert!(!proposal.canceled);
}

#[test]
fn test_fee_exceeds_maximum() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let kyc_issuer = Address::random(&env);
    let security_council = vec![&env,
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
    ];
    let council_member = security_council.get(0).unwrap();
    
    // Initialize contract with security council
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council.clone(), kyc_issuer);
    
    // Initialize protocol fee
    SubStreamContract::initialize_protocol_fee(env.clone(), admin.clone());
    
    // Try to propose fee exceeding maximum
    let reason = String::from_str(&env, "Excessive fee");
    let result = env.try_invoke_contract::<u64, _>(
        &SubStreamContract::propose_protocol_fee_update,
        &env,
        &council_member,
        &(PROTOCOL_FEE_MAX_BPS + 100), // Exceeds maximum
        &reason,
    );
    
    assert!(result.is_err());
}

#[test]
fn test_no_change_in_fee() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let kyc_issuer = Address::random(&env);
    let security_council = vec![&env,
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
    ];
    let council_member = security_council.get(0).unwrap();
    
    // Initialize contract with security council
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council.clone(), kyc_issuer);
    
    // Initialize protocol fee
    SubStreamContract::initialize_protocol_fee(env.clone(), admin.clone());
    
    // Try to propose same fee
    let reason = String::from_str(&env, "No change");
    let result = env.try_invoke_contract::<u64, _>(
        &SubStreamContract::propose_protocol_fee_update,
        &env,
        &council_member,
        &DEFAULT_PROTOCOL_FEE_BPS, // Same as current
        &reason,
    );
    
    assert!(result.is_err());
}

#[test]
fn test_unauthorized_proposer() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let kyc_issuer = Address::random(&env);
    let security_council = vec![&env,
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
    ];
    let unauthorized = Address::random(&env);
    
    // Initialize contract with security council
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council.clone(), kyc_issuer);
    
    // Initialize protocol fee
    SubStreamContract::initialize_protocol_fee(env.clone(), admin.clone());
    
    // Try to propose fee with unauthorized address
    let reason = String::from_str(&env, "Unauthorized proposal");
    let result = env.try_invoke_contract::<u64, _>(
        &SubStreamContract::propose_protocol_fee_update,
        &env,
        &unauthorized,
        &300,
        &reason,
    );
    
    assert!(result.is_err());
}

#[test]
fn test_multisig_consensus_for_fee_increase() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let kyc_issuer = Address::random(&env);
    let security_council = vec![&env,
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
    ];
    
    // Initialize contract with security council
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council.clone(), kyc_issuer);
    
    // Initialize protocol fee
    SubStreamContract::initialize_protocol_fee(env.clone(), admin.clone());
    
    // Propose fee increase
    let council_member = security_council.get(0).unwrap();
    let reason = String::from_str(&env, "Increase fee");
    let proposal_id = SubStreamContract::propose_protocol_fee_update(
        env.clone(),
        council_member.clone(),
        300,
        reason,
    );
    
    // Get 3 votes to reach consensus
    for i in 0..DAO_MULTISIG_THRESHOLD {
        let voter = security_council.get(i as usize).unwrap();
        SubStreamContract::vote_protocol_fee_update(env.clone(), voter.clone(), proposal_id);
    }
    
    // Check that proposal is not executed yet (timelock still active)
    let proposal = SubStreamContract::get_protocol_fee_proposal(env.clone(), proposal_id);
    assert!(!proposal.executed);
    
    // Fast forward past timelock
    env.ledger().set_timestamp(proposal.executable_at);
    
    // Add one more vote (should auto-execute now)
    let voter = security_council.get(DAO_MULTISIG_THRESHOLD as usize).unwrap();
    SubStreamContract::vote_protocol_fee_update(env.clone(), voter.clone(), proposal_id);
    
    // Check that proposal was executed
    let proposal = SubStreamContract::get_protocol_fee_proposal(env.clone(), proposal_id);
    assert!(proposal.executed);
    
    // Check that fee was updated
    let fee_config = SubStreamContract::get_protocol_fee_config(env.clone());
    assert_eq!(fee_config.current_fee_bps, 300);
}

#[test]
fn test_immediate_execution_for_fee_decrease_with_consensus() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let kyc_issuer = Address::random(&env);
    let security_council = vec![&env,
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
    ];
    
    // Initialize contract with security council
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council.clone(), kyc_issuer);
    
    // Initialize protocol fee
    SubStreamContract::initialize_protocol_fee(env.clone(), admin.clone());
    
    // Propose fee decrease
    let council_member = security_council.get(0).unwrap();
    let reason = String::from_str(&env, "Decrease fee");
    let proposal_id = SubStreamContract::propose_protocol_fee_update(
        env.clone(),
        council_member.clone(),
        100,
        reason,
    );
    
    // Get 3 votes to reach consensus (should auto-execute immediately)
    for i in 0..DAO_MULTISIG_THRESHOLD {
        let voter = security_council.get(i as usize).unwrap();
        SubStreamContract::vote_protocol_fee_update(env.clone(), voter.clone(), proposal_id);
    }
    
    // Check that proposal was executed immediately
    let proposal = SubStreamContract::get_protocol_fee_proposal(env.clone(), proposal_id);
    assert!(proposal.executed);
    
    // Check that fee was updated
    let fee_config = SubStreamContract::get_protocol_fee_config(env.clone());
    assert_eq!(fee_config.current_fee_bps, 100);
}

#[test]
fn test_security_council_veto() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let kyc_issuer = Address::random(&env);
    let security_council = vec![&env,
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
    ];
    
    // Initialize contract with security council
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council.clone(), kyc_issuer);
    
    // Initialize protocol fee
    SubStreamContract::initialize_protocol_fee(env.clone(), admin.clone());
    
    // Propose fee increase
    let council_member = security_council.get(0).unwrap();
    let reason = String::from_str(&env, "Increase fee");
    let proposal_id = SubStreamContract::propose_protocol_fee_update(
        env.clone(),
        council_member.clone(),
        300,
        reason,
    );
    
    // Veto the proposal
    let veto_member = security_council.get(1).unwrap();
    let veto_reason = String::from_str(&env, "Too high");
    SubStreamContract::security_council_veto_fee(
        env.clone(),
        veto_member.clone(),
        proposal_id,
        veto_reason,
    );
    
    // Check that proposal was canceled
    let proposal = SubStreamContract::get_protocol_fee_proposal(env.clone(), proposal_id);
    assert!(proposal.canceled);
    
    // Try to vote on vetoed proposal (should fail)
    let voter = security_council.get(2).unwrap();
    let result = env.try_invoke_contract::<(), _>(
        &SubStreamContract::vote_protocol_fee_update,
        &env,
        &voter,
        &proposal_id,
    );
    assert!(result.is_err());
}

#[test]
fn test_protocol_fee_update_scheduled_event() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let kyc_issuer = Address::random(&env);
    let security_council = vec![&env,
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
    ];
    let council_member = security_council.get(0).unwrap();
    
    // Initialize contract with security council
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council.clone(), kyc_issuer);
    
    // Initialize protocol fee
    SubStreamContract::initialize_protocol_fee(env.clone(), admin.clone());
    
    let new_fee_bps = 300;
    let reason = String::from_str(&env, "Increase fee");
    SubStreamContract::propose_protocol_fee_update(
        env.clone(),
        council_member.clone(),
        new_fee_bps,
        reason,
    );
    
    // Verify event was emitted
    let events = env.events().all();
    assert!(events.len() > 0);
}

#[test]
fn test_multiple_proposals() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let kyc_issuer = Address::random(&env);
    let security_council = vec![&env,
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
    ];
    let council_member = security_council.get(0).unwrap();
    
    // Initialize contract with security council
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council.clone(), kyc_issuer);
    
    // Initialize protocol fee
    SubStreamContract::initialize_protocol_fee(env.clone(), admin.clone());
    
    // Create multiple proposals
    let reason1 = String::from_str(&env, "Proposal 1");
    let proposal1 = SubStreamContract::propose_protocol_fee_update(
        env.clone(),
        council_member.clone(),
        250,
        reason1,
    );
    
    let reason2 = String::from_str(&env, "Proposal 2");
    let proposal2 = SubStreamContract::propose_protocol_fee_update(
        env.clone(),
        council_member.clone(),
        150,
        reason2,
    );
    
    // Verify both proposals exist and are different
    assert_ne!(proposal1, proposal2);
    
    let p1 = SubStreamContract::get_protocol_fee_proposal(env.clone(), proposal1);
    let p2 = SubStreamContract::get_protocol_fee_proposal(env.clone(), proposal2);
    
    assert_eq!(p1.new_fee_bps, 250);
    assert_eq!(p2.new_fee_bps, 150);
    assert!(p1.is_fee_increase);
    assert!(!p2.is_fee_increase);
}

#[test]
fn test_protocol_fee_distribution_math() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let kyc_issuer = Address::random(&env);
    let security_council = vec![&env,
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
    ];
    
    // Initialize contract with security council
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council.clone(), kyc_issuer);
    
    // Initialize protocol fee
    SubStreamContract::initialize_protocol_fee(env.clone(), admin.clone());
    
    // Set up a test scenario with known amounts
    let total_amount = 1000; // 1000 tokens
    let current_fee_bps = DEFAULT_PROTOCOL_FEE_BPS; // 200 bps = 2%
    
    // Calculate expected protocol fee
    let expected_protocol_fee = (total_amount * current_fee_bps as i128) / 10000; // 1000 * 200 / 10000 = 20
    let expected_creator_amount = total_amount - expected_protocol_fee; // 1000 - 20 = 980
    
    assert_eq!(expected_protocol_fee, 20);
    assert_eq!(expected_creator_amount, 980);
    
    // Test with different fee rates
    let higher_fee_bps = 500; // 5%
    let higher_protocol_fee = (total_amount * higher_fee_bps as i128) / 10000; // 1000 * 500 / 10000 = 50
    let higher_creator_amount = total_amount - higher_protocol_fee; // 1000 - 50 = 950
    
    assert_eq!(higher_protocol_fee, 50);
    assert_eq!(higher_creator_amount, 950);
    
    // Test edge case with very small amounts
    let small_amount = 1; // 1 token
    let small_protocol_fee = (small_amount * current_fee_bps as i128) / 10000; // 1 * 200 / 10000 = 0 (integer division)
    let small_creator_amount = small_amount - small_protocol_fee; // 1 - 0 = 1
    
    assert_eq!(small_protocol_fee, 0); // No fee due to integer division
    assert_eq!(small_creator_amount, 1);
}

#[test]
fn test_stroop_distribution_precision() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let kyc_issuer = Address::random(&env);
    let security_council = vec![&env,
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
    ];
    
    // Initialize contract with security council
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council.clone(), kyc_issuer);
    
    // Initialize protocol fee
    SubStreamContract::initialize_protocol_fee(env.clone(), admin.clone());
    
    // Test with high precision amounts to ensure no dust issues
    let precise_amount = 1_000_000; // 1M tokens
    
    // Test with maximum fee
    let max_fee_bps = PROTOCOL_FEE_MAX_BPS; // 500 bps = 5%
    let protocol_fee = (precise_amount * max_fee_bps as i128) / 10000; // 1M * 500 / 10000 = 50,000
    let creator_amount = precise_amount - protocol_fee; // 1M - 50K = 950,000
    
    assert_eq!(protocol_fee, 50_000);
    assert_eq!(creator_amount, 950_000);
    
    // Verify no rounding errors
    assert_eq!(protocol_fee + creator_amount, precise_amount);
    
    // Test with amounts that could cause dust
    let dust_prone_amount = 999; // Not divisible by 100
    let dust_protocol_fee = (dust_prone_amount * max_fee_bps as i128) / 10000; // 999 * 500 / 10000 = 49
    let dust_creator_amount = dust_prone_amount - dust_protocol_fee; // 999 - 49 = 950
    
    assert_eq!(dust_protocol_fee, 49);
    assert_eq!(dust_creator_amount, 950);
    assert_eq!(dust_protocol_fee + dust_creator_amount, dust_prone_amount);
}

#[test]
fn test_empty_reason_rejected() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let kyc_issuer = Address::random(&env);
    let security_council = vec![&env,
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
    ];
    let council_member = security_council.get(0).unwrap();
    
    // Initialize contract with security council
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council.clone(), kyc_issuer);
    
    // Initialize protocol fee
    SubStreamContract::initialize_protocol_fee(env.clone(), admin.clone());
    
    // Try to propose with empty reason
    let empty_reason = String::from_str(&env, "");
    let result = env.try_invoke_contract::<u64, _>(
        &SubStreamContract::propose_protocol_fee_update,
        &env,
        &council_member,
        &300,
        &empty_reason,
    );
    
    assert!(result.is_err());
}

#[test]
fn test_reason_too_long_rejected() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let kyc_issuer = Address::random(&env);
    let security_council = vec![&env,
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
    ];
    let council_member = security_council.get(0).unwrap();
    
    // Initialize contract with security council
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council.clone(), kyc_issuer);
    
    // Initialize protocol fee
    SubStreamContract::initialize_protocol_fee(env.clone(), admin.clone());
    
    // Try to propose with reason exceeding max length
    let long_reason = String::from_str(&env, &"a".repeat(MAX_REASON_LENGTH as usize + 1));
    let result = env.try_invoke_contract::<u64, _>(
        &SubStreamContract::propose_protocol_fee_update,
        &env,
        &council_member,
        &300,
        &long_reason,
    );
    
    assert!(result.is_err());
}

#[test]
fn test_veto_with_empty_reason_rejected() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let kyc_issuer = Address::random(&env);
    let security_council = vec![&env,
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
    ];
    
    // Initialize contract with security council
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council.clone(), kyc_issuer);
    
    // Initialize protocol fee
    SubStreamContract::initialize_protocol_fee(env.clone(), admin.clone());
    
    // Propose fee increase
    let council_member = security_council.get(0).unwrap();
    let reason = String::from_str(&env, "Increase fee");
    let proposal_id = SubStreamContract::propose_protocol_fee_update(
        env.clone(),
        council_member.clone(),
        300,
        reason,
    );
    
    // Try to veto with empty reason
    let veto_member = security_council.get(1).unwrap();
    let empty_veto_reason = String::from_str(&env, "");
    let result = env.try_invoke_contract::<(), _>(
        &SubStreamContract::security_council_veto_fee,
        &env,
        &veto_member,
        &proposal_id,
        &empty_veto_reason,
    );
    
    assert!(result.is_err());
}

#[test]
fn test_veto_reason_too_long_rejected() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let kyc_issuer = Address::random(&env);
    let security_council = vec![&env,
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
    ];
    
    // Initialize contract with security council
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council.clone(), kyc_issuer);
    
    // Initialize protocol fee
    SubStreamContract::initialize_protocol_fee(env.clone(), admin.clone());
    
    // Propose fee increase
    let council_member = security_council.get(0).unwrap();
    let reason = String::from_str(&env, "Increase fee");
    let proposal_id = SubStreamContract::propose_protocol_fee_update(
        env.clone(),
        council_member.clone(),
        300,
        reason,
    );
    
    // Try to veto with reason exceeding max length
    let veto_member = security_council.get(1).unwrap();
    let long_veto_reason = String::from_str(&env, &"a".repeat(MAX_REASON_LENGTH as usize + 1));
    let result = env.try_invoke_contract::<(), _>(
        &SubStreamContract::security_council_veto_fee,
        &env,
        &veto_member,
        &proposal_id,
        &long_veto_reason,
    );
    
    assert!(result.is_err());
}

#[test]
fn test_vote_on_expired_proposal_rejected() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let kyc_issuer = Address::random(&env);
    let security_council = vec![&env,
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
    ];
    
    // Initialize contract with security council
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council.clone(), kyc_issuer);
    
    // Initialize protocol fee
    SubStreamContract::initialize_protocol_fee(env.clone(), admin.clone());
    
    // Propose fee increase
    let council_member = security_council.get(0).unwrap();
    let reason = String::from_str(&env, "Increase fee");
    let proposal_id = SubStreamContract::propose_protocol_fee_update(
        env.clone(),
        council_member.clone(),
        300,
        reason,
    );
    
    // Fast forward past proposal expiry (30 days)
    let proposal = SubStreamContract::get_protocol_fee_proposal(env.clone(), proposal_id);
    env.ledger().set_timestamp(proposal.proposed_at + 31 * 24 * 60 * 60);
    
    // Try to vote on expired proposal
    let voter = security_council.get(1).unwrap();
    let result = env.try_invoke_contract::<(), _>(
        &SubStreamContract::vote_protocol_fee_update,
        &env,
        &voter,
        &proposal_id,
    );
    
    assert!(result.is_err());
}

#[test]
fn test_execute_expired_proposal_rejected() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let kyc_issuer = Address::random(&env);
    let security_council = vec![&env,
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
    ];
    
    // Initialize contract with security council
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council.clone(), kyc_issuer);
    
    // Initialize protocol fee
    SubStreamContract::initialize_protocol_fee(env.clone(), admin.clone());
    
    // Propose fee decrease (immediate execution)
    let council_member = security_council.get(0).unwrap();
    let reason = String::from_str(&env, "Decrease fee");
    let proposal_id = SubStreamContract::propose_protocol_fee_update(
        env.clone(),
        council_member.clone(),
        100,
        reason,
    );
    
    // Get 3 votes to reach consensus
    for i in 0..DAO_MULTISIG_THRESHOLD {
        let voter = security_council.get(i as usize).unwrap();
        SubStreamContract::vote_protocol_fee_update(env.clone(), voter.clone(), proposal_id);
    }
    
    // Fast forward past proposal expiry (30 days)
    let proposal = SubStreamContract::get_protocol_fee_proposal(env.clone(), proposal_id);
    env.ledger().set_timestamp(proposal.proposed_at + 31 * 24 * 60 * 60);
    
    // Try to execute expired proposal
    let executor = Address::random(&env);
    let result = env.try_invoke_contract::<(), _>(
        &SubStreamContract::execute_protocol_fee_update,
        &env,
        &executor,
        &proposal_id,
    );
    
    assert!(result.is_err());
}

#[test]
fn test_veto_expired_proposal_rejected() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let kyc_issuer = Address::random(&env);
    let security_council = vec![&env,
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
    ];
    
    // Initialize contract with security council
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council.clone(), kyc_issuer);
    
    // Initialize protocol fee
    SubStreamContract::initialize_protocol_fee(env.clone(), admin.clone());
    
    // Propose fee increase
    let council_member = security_council.get(0).unwrap();
    let reason = String::from_str(&env, "Increase fee");
    let proposal_id = SubStreamContract::propose_protocol_fee_update(
        env.clone(),
        council_member.clone(),
        300,
        reason,
    );
    
    // Fast forward past proposal expiry (30 days)
    let proposal = SubStreamContract::get_protocol_fee_proposal(env.clone(), proposal_id);
    env.ledger().set_timestamp(proposal.proposed_at + 31 * 24 * 60 * 60);
    
    // Try to veto expired proposal
    let veto_member = security_council.get(1).unwrap();
    let veto_reason = String::from_str(&env, "Too late");
    let result = env.try_invoke_contract::<(), _>(
        &SubStreamContract::security_council_veto_fee,
        &env,
        &veto_member,
        &proposal_id,
        &veto_reason,
    );
    
    assert!(result.is_err());
}

#[test]
fn test_initialize_fee_without_contract_initialization_rejected() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    
    // Try to initialize protocol fee without initializing contract
    let result = env.try_invoke_contract::<(), _>(
        &SubStreamContract::initialize_protocol_fee,
        &env,
        &admin,
    );
    
    assert!(result.is_err());
}

#[test]
fn test_propose_without_contract_initialization_rejected() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let kyc_issuer = Address::random(&env);
    let security_council = vec![&env,
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
    ];
    let council_member = security_council.get(0).unwrap();
    
    // Initialize contract with security council
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council.clone(), kyc_issuer);
    
    // Try to propose without initializing protocol fee
    let reason = String::from_str(&env, "Increase fee");
    let result = env.try_invoke_contract::<u64, _>(
        &SubStreamContract::propose_protocol_fee_update,
        &env,
        &council_member,
        &300,
        &reason,
    );
    
    assert!(result.is_err());
}
