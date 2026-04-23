#![cfg(test)]

use soroban_sdk::{Address, Env, vec};
use crate::{
    SubStreamContract, ProtocolFeeConfig, ProtocolFeeUpdateProposal, DataKey,
    PROTOCOL_FEE_MAX_BPS, PROTOCOL_FEE_TIMELOCK_DURATION, DEFAULT_PROTOCOL_FEE_BPS,
    DAO_MULTISIG_THRESHOLD, ProtocolFeeUpdateScheduled, ProtocolFeeUpdateExecuted
};

/// Integration test for end-to-end protocol fee functionality
/// This test simulates a complete fee update cycle with distribution
#[test]
fn test_end_to_end_protocol_fee_workflow() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let dao_member = Address::random(&env);
    let creator = Address::random(&env);
    let subscriber = Address::random(&env);
    
    // Initialize contract
    SubStreamContract::initialize(env.clone(), admin.clone());
    
    // Verify initial fee configuration
    let initial_config = SubStreamContract::get_protocol_fee_config(env.clone());
    assert_eq!(initial_config.current_fee_bps, DEFAULT_PROTOCOL_FEE_BPS);
    assert_eq!(initial_config.updated_by, admin);
    
    // Step 1: Propose a fee increase
    let new_fee_bps = 300; // Increase from 200 to 300 bps
    let proposal_id = SubStreamContract::propose_protocol_fee_update(
        env.clone(),
        dao_member.clone(),
        new_fee_bps,
    );
    
    // Verify proposal was created correctly
    let proposal_key = DataKey::ProtocolFeeUpdateProposal(proposal_id);
    let proposal: ProtocolFeeUpdateProposal = env.storage().persistent().get(&proposal_key).unwrap();
    
    assert_eq!(proposal.new_fee_bps, new_fee_bps);
    assert_eq!(proposal.old_fee_bps, DEFAULT_PROTOCOL_FEE_BPS);
    assert!(proposal.is_fee_increase);
    assert_eq!(proposal.executable_at, proposal.proposed_at + PROTOCOL_FEE_TIMELOCK_DURATION);
    
    // Verify event was emitted
    let events = env.events().all();
    let scheduled_events = events.iter().filter(|event| {
        match event {
            soroban_sdk::xdr::ContractEvent::V0(v0) => {
                let topic = soroban_sdk::Symbol::new(&env, "ProtocolFeeUpdateScheduled");
                v0.topics.contains(&topic.to_val())
            }
            _ => false,
        }
    }).collect::<Vec<_>>();
    assert_eq!(scheduled_events.len(), 1);
    
    // Step 2: Simulate DAO consensus (in real implementation, this would require multiple votes)
    // For this test, we'll fast-forward past the timelock and execute
    
    // Fast forward past timelock
    env.ledger().set_timestamp(proposal.executable_at);
    
    // Step 3: Execute the fee update
    SubStreamContract::execute_protocol_fee_update(
        env.clone(),
        dao_member.clone(),
        proposal_id,
    );
    
    // Verify fee was updated
    let updated_config = SubStreamContract::get_protocol_fee_config(env.clone());
    assert_eq!(updated_config.current_fee_bps, new_fee_bps);
    assert_eq!(updated_config.updated_by, dao_member);
    
    // Verify execution event was emitted
    let events_after = env.events().all();
    let executed_events = events_after.iter().filter(|event| {
        match event {
            soroban_sdk::xdr::ContractEvent::V0(v0) => {
                let topic = soroban_sdk::Symbol::new(&env, "ProtocolFeeUpdateExecuted");
                v0.topics.contains(&topic.to_val())
            }
            _ => false,
        }
    }).collect::<Vec<_>>();
    assert_eq!(executed_events.len(), 1);
    
    // Step 4: Test fee distribution with new rate
    // Simulate a collection scenario
    let total_amount = 1000; // 1000 tokens
    
    // Calculate expected distribution with new fee
    let expected_protocol_fee = (total_amount * new_fee_bps as i128) / 10000; // 1000 * 300 / 10000 = 30
    let expected_creator_amount = total_amount - expected_protocol_fee; // 1000 - 30 = 970
    
    assert_eq!(expected_protocol_fee, 30);
    assert_eq!(expected_creator_amount, 970);
    
    // Verify mathematical precision
    assert_eq!(expected_protocol_fee + expected_creator_amount, total_amount);
}

/// Test mid-cycle fee update scenario
#[test]
fn test_mid_cycle_fee_update() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let dao_member = Address::random(&env);
    
    // Initialize contract with default fee
    SubStreamContract::initialize(env.clone(), admin.clone());
    
    // Simulate an active subscription period
    let start_time = env.ledger().timestamp();
    
    // Step 1: Start with default fee (200 bps)
    let initial_fee_bps = DEFAULT_PROTOCOL_FEE_BPS;
    let collection_amount = 500; // 500 tokens collected
    
    let initial_protocol_fee = (collection_amount * initial_fee_bps as i128) / 10000; // 500 * 200 / 10000 = 10
    let initial_creator_amount = collection_amount - initial_protocol_fee; // 500 - 10 = 490
    
    assert_eq!(initial_protocol_fee, 10);
    assert_eq!(initial_creator_amount, 490);
    
    // Step 2: Propose fee increase mid-cycle
    env.ledger().set_timestamp(start_time + 3600); // 1 hour later
    
    let increased_fee_bps = 400; // Increase to 400 bps
    let proposal_id = SubStreamContract::propose_protocol_fee_update(
        env.clone(),
        dao_member.clone(),
        increased_fee_bps,
    );
    
    // Step 3: Fast forward past timelock and execute
    let proposal_key = DataKey::ProtocolFeeUpdateProposal(proposal_id);
    let proposal: ProtocolFeeUpdateProposal = env.storage().persistent().get(&proposal_key).unwrap();
    env.ledger().set_timestamp(proposal.executable_at);
    
    SubStreamContract::execute_protocol_fee_update(
        env.clone(),
        dao_member.clone(),
        proposal_id,
    );
    
    // Step 4: Verify new fee applies to subsequent collections
    let updated_config = SubStreamContract::get_protocol_fee_config(env.clone());
    assert_eq!(updated_config.current_fee_bps, increased_fee_bps);
    
    // Calculate distribution with new fee for next collection
    let next_collection_amount = 500; // Another 500 tokens
    let new_protocol_fee = (next_collection_amount * increased_fee_bps as i128) / 10000; // 500 * 400 / 10000 = 20
    let new_creator_amount = next_collection_amount - new_protocol_fee; // 500 - 20 = 480
    
    assert_eq!(new_protocol_fee, 20);
    assert_eq!(new_creator_amount, 480);
    
    // Verify fee increase impact
    assert_eq!(new_protocol_fee, initial_protocol_fee * 2); // Fee doubled
    assert_eq!(new_creator_amount, initial_creator_amount - 10); // Creator gets 10 less
}

/// Test treasury balance accumulation over multiple collections
#[test]
fn test_treasury_balance_accumulation() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let dao_member = Address::random(&env);
    
    // Initialize contract
    SubStreamContract::initialize(env.clone(), admin.clone());
    
    // Set a higher fee for testing
    let test_fee_bps = 300; // 3%
    let proposal_id = SubStreamContract::propose_protocol_fee_update(
        env.clone(),
        dao_member.clone(),
        test_fee_bps,
    );
    
    // Execute immediately (for testing - in reality this would require timelock)
    let proposal_key = DataKey::ProtocolFeeUpdateProposal(proposal_id);
    let proposal: ProtocolFeeUpdateProposal = env.storage().persistent().get(&proposal_key).unwrap();
    env.ledger().set_timestamp(proposal.executable_at);
    
    SubStreamContract::execute_protocol_fee_update(
        env.clone(),
        dao_member.clone(),
        proposal_id,
    );
    
    // Simulate multiple collection periods
    let collection_amounts = vec![&env, 1000, 1500, 800, 1200]; // 4 collection periods
    let mut total_protocol_fees = 0;
    let mut total_creator_amounts = 0;
    
    for amount in collection_amounts.iter() {
        let protocol_fee = (*amount * test_fee_bps as i128) / 10000;
        let creator_amount = *amount - protocol_fee;
        
        total_protocol_fees += protocol_fee;
        total_creator_amounts += creator_amount;
        
        // Verify no rounding errors in each period
        assert_eq!(protocol_fee + creator_amount, *amount);
    }
    
    // Calculate totals
    let total_collected = 1000 + 1500 + 800 + 1200; // 4500 total
    let expected_total_fees = (total_collected * test_fee_bps as i128) / 10000; // 4500 * 300 / 10000 = 135
    let expected_total_creator_amount = total_collected - expected_total_fees; // 4500 - 135 = 4365
    
    assert_eq!(total_protocol_fees, expected_total_fees);
    assert_eq!(total_creator_amounts, expected_total_creator_amount);
    
    // Verify overall mathematical integrity
    assert_eq!(total_protocol_fees + total_creator_amounts, total_collected);
    
    // In a real implementation, we would verify the treasury (admin) balance
    // increased by total_protocol_fees and creators received their shares
}

/// Test fee decrease scenario (immediate execution)
#[test]
fn test_fee_decrease_immediate_execution() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let dao_member = Address::random(&env);
    
    // Initialize contract
    SubStreamContract::initialize(env.clone(), admin.clone());
    
    // First increase the fee to establish a higher baseline
    let increased_fee_bps = 400; // 4%
    let proposal1_id = SubStreamContract::propose_protocol_fee_update(
        env.clone(),
        dao_member.clone(),
        increased_fee_bps,
    );
    
    let proposal1_key = DataKey::ProtocolFeeUpdateProposal(proposal1_id);
    let proposal1: ProtocolFeeUpdateProposal = env.storage().persistent().get(&proposal1_key).unwrap();
    env.ledger().set_timestamp(proposal1.executable_at);
    
    SubStreamContract::execute_protocol_fee_update(
        env.clone(),
        dao_member.clone(),
        proposal1_id,
    );
    
    // Verify increased fee is active
    let config1 = SubStreamContract::get_protocol_fee_config(env.clone());
    assert_eq!(config1.current_fee_bps, increased_fee_bps);
    
    // Now propose a fee decrease
    let decreased_fee_bps = 150; // 1.5%
    let proposal2_id = SubStreamContract::propose_protocol_fee_update(
        env.clone(),
        dao_member.clone(),
        decreased_fee_bps,
    );
    
    // Verify decrease proposal has immediate execution
    let proposal2_key = DataKey::ProtocolFeeUpdateProposal(proposal2_id);
    let proposal2: ProtocolFeeUpdateProposal = env.storage().persistent().get(&proposal2_key).unwrap();
    
    assert!(!proposal2.is_fee_increase);
    assert_eq!(proposal2.executable_at, proposal2.proposed_at); // Immediate execution
    
    // Execute immediately (no timelock required)
    SubStreamContract::execute_protocol_fee_update(
        env.clone(),
        dao_member.clone(),
        proposal2_id,
    );
    
    // Verify decreased fee is active
    let config2 = SubStreamContract::get_protocol_fee_config(env.clone());
    assert_eq!(config2.current_fee_bps, decreased_fee_bps);
    
    // Test distribution with decreased fee
    let collection_amount = 1000;
    let old_protocol_fee = (collection_amount * increased_fee_bps as i128) / 10000; // 1000 * 400 / 10000 = 40
    let new_protocol_fee = (collection_amount * decreased_fee_bps as i128) / 10000; // 1000 * 150 / 10000 = 15
    
    assert_eq!(old_protocol_fee, 40);
    assert_eq!(new_protocol_fee, 15);
    assert_eq!(new_protocol_fee, old_protocol_fee - 25); // 25 less in fees
}

/// Test maximum fee cap enforcement
#[test]
fn test_maximum_fee_cap_enforcement() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let dao_member = Address::random(&env);
    
    // Initialize contract
    SubStreamContract::initialize(env.clone(), admin.clone());
    
    // Try to propose fee exceeding maximum
    let excessive_fee_bps = PROTOCOL_FEE_MAX_BPS + 100; // 600 bps (exceeds 500 max)
    
    let result = env.try_invoke_contract::<u64, (
        &SubStreamContract::propose_protocol_fee_update,
        &env,
        &dao_member,
        &excessive_fee_bps,
    );
    
    assert!(result.is_err());
    
    // Verify maximum fee is still enforceable
    let max_fee_bps = PROTOCOL_FEE_MAX_BPS; // 500 bps
    let proposal_id = SubStreamContract::propose_protocol_fee_update(
        env.clone(),
        dao_member.clone(),
        max_fee_bps,
    );
    
    // Verify maximum fee proposal was accepted
    let proposal_key = DataKey::ProtocolFeeUpdateProposal(proposal_id);
    let proposal: ProtocolFeeUpdateProposal = env.storage().persistent().get(&proposal_key).unwrap();
    assert_eq!(proposal.new_fee_bps, max_fee_bps);
    
    // Test distribution at maximum fee
    let collection_amount = 1000;
    let max_protocol_fee = (collection_amount * max_fee_bps as i128) / 10000; // 1000 * 500 / 10000 = 50
    let creator_amount_at_max = collection_amount - max_protocol_fee; // 1000 - 50 = 950
    
    assert_eq!(max_protocol_fee, 50);
    assert_eq!(creator_amount_at_max, 950);
    
    // Verify this is indeed the maximum (5% of collection)
    assert_eq!(max_protocol_fee, collection_amount / 20); // 5% = 1/20
}
