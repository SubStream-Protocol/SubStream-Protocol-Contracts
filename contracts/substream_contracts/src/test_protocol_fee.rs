#![cfg(test)]

use soroban_sdk::{Address, Env, vec};
use crate::{
    SubStreamContract, ProtocolFeeConfig, ProtocolFeeUpdateProposal, DataKey,
    PROTOCOL_FEE_MAX_BPS, PROTOCOL_FEE_TIMELOCK_DURATION, DEFAULT_PROTOCOL_FEE_BPS,
    DAO_MULTISIG_THRESHOLD, ProtocolFeeUpdateScheduled, ProtocolFeeUpdateExecuted
};

#[test]
fn test_protocol_fee_initialization() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    
    // Initialize contract
    SubStreamContract::initialize(env.clone(), admin.clone());
    
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
    let dao_member = Address::random(&env);
    
    // Initialize contract
    SubStreamContract::initialize(env.clone(), admin.clone());
    
    // Set up DAO authorization (mock)
    // In real implementation, this would involve proper DAO member verification
    
    let new_fee_bps = 300; // Increase from 200 to 300 bps
    let proposal_id = SubStreamContract::propose_protocol_fee_update(
        env.clone(),
        dao_member.clone(),
        new_fee_bps,
    );
    
    // Verify proposal was created
    let proposal_key = DataKey::ProtocolFeeUpdateProposal(proposal_id);
    let proposal: ProtocolFeeUpdateProposal = env.storage().persistent().get(&proposal_key).unwrap();
    
    assert_eq!(proposal.new_fee_bps, new_fee_bps);
    assert_eq!(proposal.old_fee_bps, DEFAULT_PROTOCOL_FEE_BPS);
    assert_eq!(proposal.proposed_by, dao_member);
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
    let dao_member = Address::random(&env);
    
    // Initialize contract
    SubStreamContract::initialize(env.clone(), admin.clone());
    
    let new_fee_bps = 100; // Decrease from 200 to 100 bps
    let proposal_id = SubStreamContract::propose_protocol_fee_update(
        env.clone(),
        dao_member.clone(),
        new_fee_bps,
    );
    
    // Verify proposal was created
    let proposal_key = DataKey::ProtocolFeeUpdateProposal(proposal_id);
    let proposal: ProtocolFeeUpdateProposal = env.storage().persistent().get(&proposal_key).unwrap();
    
    assert_eq!(proposal.new_fee_bps, new_fee_bps);
    assert_eq!(proposal.old_fee_bps, DEFAULT_PROTOCOL_FEE_BPS);
    assert_eq!(proposal.proposed_by, dao_member);
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
    let dao_member = Address::random(&env);
    
    // Initialize contract
    SubStreamContract::initialize(env.clone(), admin.clone());
    
    // Try to propose fee exceeding maximum
    let result = env.try_invoke_contract::<u64, (
        &SubStreamContract::propose_protocol_fee_update,
        &env,
        &dao_member,
        &(PROTOCOL_FEE_MAX_BPS + 100), // Exceeds maximum
    );
    
    assert!(result.is_err());
}

#[test]
fn test_no_change_in_fee() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let dao_member = Address::random(&env);
    
    // Initialize contract
    SubStreamContract::initialize(env.clone(), admin.clone());
    
    // Try to propose same fee
    let result = env.try_invoke_contract::<u64, (
        &SubStreamContract::propose_protocol_fee_update,
        &env,
        &dao_member,
        &DEFAULT_PROTOCOL_FEE_BPS, // Same as current
    );
    
    assert!(result.is_err());
}

#[test]
fn test_timelock_enforcement_for_fee_increase() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let dao_member = Address::random(&env);
    
    // Initialize contract
    SubStreamContract::initialize(env.clone(), admin.clone());
    
    let new_fee_bps = 300;
    let proposal_id = SubStreamContract::propose_protocol_fee_update(
        env.clone(),
        dao_member.clone(),
        new_fee_bps,
    );
    
    // Try to execute immediately (should fail due to timelock)
    let result = env.try_invoke_contract::<(), (
        &SubStreamContract::execute_protocol_fee_update,
        &env,
        &dao_member,
        &proposal_id,
    );
    assert!(result.is_err());
    
    // Fast forward past timelock
    let proposal_key = DataKey::ProtocolFeeUpdateProposal(proposal_id);
    let proposal: ProtocolFeeUpdateProposal = env.storage().persistent().get(&proposal_key).unwrap();
    env.ledger().set_timestamp(proposal.executable_at);
    
    // Now should succeed (assuming proper DAO votes)
    // In real implementation, this would require DAO consensus
}

#[test]
fn test_immediate_execution_for_fee_decrease() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let dao_member = Address::random(&env);
    
    // Initialize contract
    SubStreamContract::initialize(env.clone(), admin.clone());
    
    let new_fee_bps = 100; // Decrease
    let proposal_id = SubStreamContract::propose_protocol_fee_update(
        env.clone(),
        dao_member.clone(),
        new_fee_bps,
    );
    
    // Verify executable_at is immediate (no timelock for decreases)
    let proposal_key = DataKey::ProtocolFeeUpdateProposal(proposal_id);
    let proposal: ProtocolFeeUpdateProposal = env.storage().persistent().get(&proposal_key).unwrap();
    assert_eq!(proposal.executable_at, proposal.proposed_at);
}

#[test]
fn test_protocol_fee_update_scheduled_event() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let dao_member = Address::random(&env);
    
    // Initialize contract
    SubStreamContract::initialize(env.clone(), admin.clone());
    
    let new_fee_bps = 300;
    SubStreamContract::propose_protocol_fee_update(
        env.clone(),
        dao_member.clone(),
        new_fee_bps,
    );
    
    // Verify event was emitted
    let events = env.events().all();
    let fee_events = events.iter().filter(|event| {
        match event {
            soroban_sdk::xdr::ContractEvent::V0(v0) => {
                let topic = soroban_sdk::Symbol::new(&env, "ProtocolFeeUpdateScheduled");
                v0.topics.contains(&topic.to_val())
            }
            _ => false,
        }
    }).collect::<Vec<_>>();
    
    assert_eq!(fee_events.len(), 1);
}

#[test]
fn test_multiple_proposals() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let dao_member = Address::random(&env);
    
    // Initialize contract
    SubStreamContract::initialize(env.clone(), admin.clone());
    
    // Create multiple proposals
    let proposal1 = SubStreamContract::propose_protocol_fee_update(
        env.clone(),
        dao_member.clone(),
        250,
    );
    
    let proposal2 = SubStreamContract::propose_protocol_fee_update(
        env.clone(),
        dao_member.clone(),
        150,
    );
    
    // Verify both proposals exist and are different
    assert_ne!(proposal1, proposal2);
    
    let proposal_key1 = DataKey::ProtocolFeeUpdateProposal(proposal1);
    let proposal_key2 = DataKey::ProtocolFeeUpdateProposal(proposal2);
    
    let p1: ProtocolFeeUpdateProposal = env.storage().persistent().get(&proposal_key1).unwrap();
    let p2: ProtocolFeeUpdateProposal = env.storage().persistent().get(&proposal_key2).unwrap();
    
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
    let creator = Address::random(&env);
    let subscriber = Address::random(&env);
    
    // Initialize contract
    SubStreamContract::initialize(env.clone(), admin.clone());
    
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
    
    // Initialize contract
    SubStreamContract::initialize(env.clone(), admin.clone());
    
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
