#![cfg(test)]

use soroban_sdk::{Address, Env, vec};
use crate::{
    SubStreamContract, RegistryUpdateProposal, RegistryUpdateType, SecurityCouncilMember,
    RegistryUpdateProposed, RegistryUpdateExecuted, RegistryUpdateCanceled, SecurityCouncilVetoed,
    DataKey, TIMELOCK_DURATION, DAO_MULTISIG_THRESHOLD, SECURITY_COUNCIL_SIZE
};

#[test]
fn test_48_hour_timelock_enforcement() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let merchant = Address::random(&env);
    
    // Initialize contract
    SubStreamContract::initialize(env.clone(), admin.clone());
    
    // Set up security council
    let council_members = vec![
        &env,
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
    ];
    SubStreamContract::initialize_security_council(env.clone(), admin.clone(), council_members.clone());
    
    // Create registry update proposal (no emergency bypass)
    let proposal_id = SubStreamContract::propose_registry_update(
        env.clone(),
        council_members.get(0).unwrap(),
        merchant.clone(),
        RegistryUpdateType::WhitelistMerchant,
        soroban_sdk::String::from_str(&env, "Whitelist legitimate merchant"),
        false, // No emergency bypass
    );
    
    // Get proposal and verify timelock
    let proposal_key = DataKey::RegistryUpdateProposal(proposal_id);
    let proposal: RegistryUpdateProposal = env.storage().persistent().get(&proposal_key).unwrap();
    
    let expected_executable_at = proposal.proposed_at + TIMELOCK_DURATION;
    assert_eq!(proposal.executable_at, expected_executable_at);
    assert!(!proposal.emergency_bypass);
    
    // Try to execute immediately (should fail - timelock not expired)
    let result = env.try_invoke_contract::<(), (
        &SubStreamContract::execute_registry_update,
        &env,
        council_members.get(0).unwrap(),
        proposal_id,
    );
    assert!(result.is_err());
    
    // Get 3 votes to reach consensus
    SubStreamContract::vote_registry_update(env.clone(), council_members.get(0).unwrap(), proposal_id);
    SubStreamContract::vote_registry_update(env.clone(), council_members.get(1).unwrap(), proposal_id);
    SubStreamContract::vote_registry_update(env.clone(), council_members.get(2).unwrap(), proposal_id);
    
    // Still should not execute because timelock not expired
    let proposal_after_votes: RegistryUpdateProposal = env.storage().persistent().get(&proposal_key).unwrap();
    assert!(!proposal_after_votes.executed);
    
    // Fast forward time to just before timelock expires (47 hours 59 minutes)
    let almost_executable_time = proposal.proposed_at + TIMELOCK_DURATION - 60;
    env.ledger().set_timestamp(almost_executable_time);
    
    // Try to execute (should still fail)
    let result = env.try_invoke_contract::<(), (
        &SubStreamContract::execute_registry_update,
        &env,
        council_members.get(0).unwrap(),
        proposal_id,
    );
    assert!(result.is_err());
    
    // Fast forward to exactly executable time
    env.ledger().set_timestamp(expected_executable_time);
    
    // Now should execute successfully
    SubStreamContract::execute_registry_update(env.clone(), council_members.get(0).unwrap(), proposal_id);
    
    // Verify proposal executed
    let executed_proposal: RegistryUpdateProposal = env.storage().persistent().get(&proposal_key).unwrap();
    assert!(executed_proposal.executed);
    
    // Verify merchant is now whitelisted
    assert!(SubStreamContract::is_merchant_verified(env.clone(), merchant.clone()));
}

#[test]
fn test_emergency_bypass_immediate_execution() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let merchant = Address::random(&env);
    
    // Initialize contract
    SubStreamContract::initialize(env.clone(), admin.clone());
    
    // Set up security council
    let council_members = vec![
        &env,
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
    ];
    SubStreamContract::initialize_security_council(env.clone(), admin.clone(), council_members.clone());
    
    // Create emergency proposal (with bypass)
    let proposal_id = SubStreamContract::propose_registry_update(
        env.clone(),
        admin.clone(), // Only admin can use emergency bypass
        merchant.clone(),
        RegistryUpdateType::BlacklistMerchant,
        soroban_sdk::String::from_str(&env, "Emergency blacklist for severe scam"),
        true, // Emergency bypass
    );
    
    // Verify emergency bypass proposal
    let proposal_key = DataKey::RegistryUpdateProposal(proposal_id);
    let proposal: RegistryUpdateProposal = env.storage().persistent().get(&proposal_key).unwrap();
    assert!(proposal.emergency_bypass);
    assert_eq!(proposal.executable_at, proposal.proposed_at); // Immediate execution
    
    // Get 3 votes for consensus
    SubStreamContract::vote_registry_update(env.clone(), council_members.get(0).unwrap(), proposal_id);
    SubStreamContract::vote_registry_update(env.clone(), council_members.get(1).unwrap(), proposal_id);
    SubStreamContract::vote_registry_update(env.clone(), council_members.get(2).unwrap(), proposal_id);
    
    // Should execute immediately (no timelock)
    let executed_proposal: RegistryUpdateProposal = env.storage().persistent().get(&proposal_key).unwrap();
    assert!(executed_proposal.executed);
    
    // Verify merchant is blacklisted
    assert!(!SubStreamContract::is_merchant_verified(env.clone(), merchant.clone()));
}

#[test]
fn test_multisig_consensus_prevents_unilateral_control() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let merchant = Address::random(&env);
    
    // Initialize contract
    SubStreamContract::initialize(env.clone(), admin.clone());
    
    // Set up security council
    let council_members = vec![
        &env,
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
    ];
    SubStreamContract::initialize_security_council(env.clone(), admin.clone(), council_members.clone());
    
    // Create proposal
    let proposal_id = SubStreamContract::propose_registry_update(
        env.clone(),
        council_members.get(0).unwrap(),
        merchant.clone(),
        RegistryUpdateType::WhitelistMerchant,
        soroban_sdk::String::from_str(&env, "Test proposal"),
        false,
    );
    
    // Only 1 vote (below threshold of 3)
    SubStreamContract::vote_registry_update(env.clone(), council_members.get(0).unwrap(), proposal_id);
    
    // Fast forward past timelock
    let proposal_key = DataKey::RegistryUpdateProposal(proposal_id);
    let proposal: RegistryUpdateProposal = env.storage().persistent().get(&proposal_key).unwrap();
    env.ledger().set_timestamp(proposal.executable_at);
    
    // Try to execute with insufficient consensus (should fail)
    let result = env.try_invoke_contract::<(), (
        &SubStreamContract::execute_registry_update,
        &env,
        council_members.get(0).unwrap(),
        proposal_id,
    );
    assert!(result.is_err());
    
    // Add second vote (still insufficient)
    SubStreamContract::vote_registry_update(env.clone(), council_members.get(1).unwrap(), proposal_id);
    
    // Still should fail
    let result = env.try_invoke_contract::<(), (
        &SubStreamContract::execute_registry_update,
        &env,
        council_members.get(0).unwrap(),
        proposal_id,
    );
    assert!(result.is_err());
    
    // Add third vote (reaches threshold)
    SubStreamContract::vote_registry_update(env.clone(), council_members.get(2).unwrap(), proposal_id);
    
    // Should now execute
    let executed_proposal: RegistryUpdateProposal = env.storage().persistent().get(&proposal_key).unwrap();
    assert!(executed_proposal.executed);
}

#[test]
fn test_security_council_veto_pending_proposal() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let merchant = Address::random(&env);
    
    // Initialize contract
    SubStreamContract::initialize(env.clone(), admin.clone());
    
    // Set up security council
    let council_members = vec![
        &env,
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
    ];
    SubStreamContract::initialize_security_council(env.clone(), admin.clone(), council_members.clone());
    
    // Create proposal
    let proposal_id = SubStreamContract::propose_registry_update(
        env.clone(),
        council_members.get(0).unwrap(),
        merchant.clone(),
        RegistryUpdateType::WhitelistMerchant,
        soroban_sdk::String::from_str(&env, "Suspicious proposal"),
        false,
    );
    
    // Get 2 votes (not yet at threshold)
    SubStreamContract::vote_registry_update(env.clone(), council_members.get(0).unwrap(), proposal_id);
    SubStreamContract::vote_registry_update(env.clone(), council_members.get(1).unwrap(), proposal_id);
    
    // Security council member vetoes the proposal
    SubStreamContract::security_council_veto(
        env.clone(),
        council_members.get(3).unwrap(),
        proposal_id,
        soroban_sdk::String::from_str(&env, "Proposal appears malicious"),
    );
    
    // Verify proposal is canceled
    let proposal_key = DataKey::RegistryUpdateProposal(proposal_id);
    let proposal: RegistryUpdateProposal = env.storage().persistent().get(&proposal_key).unwrap();
    assert!(proposal.canceled);
    assert!(!proposal.executed);
    
    // Try to add more votes (should fail - proposal canceled)
    let result = env.try_invoke_contract::<(), (
        &SubStreamContract::vote_registry_update,
        &env,
        council_members.get(2).unwrap(),
        proposal_id,
    );
    assert!(result.is_err());
    
    // Try to execute (should fail - proposal canceled)
    let result = env.try_invoke_contract::<(), (
        &SubStreamContract::execute_registry_update,
        &env,
        council_members.get(0).unwrap(),
        proposal_id,
    );
    assert!(result.is_err());
    
    // Verify merchant is not whitelisted
    assert!(!SubStreamContract::is_merchant_verified(env.clone(), merchant.clone()));
}

#[test]
fn test_registry_update_proposed_event() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let merchant = Address::random(&env);
    
    // Initialize contract
    SubStreamContract::initialize(env.clone(), admin.clone());
    
    // Set up security council
    let council_members = vec![
        &env,
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
    ];
    SubStreamContract::initialize_security_council(env.clone(), admin.clone(), council_members.clone());
    
    // Create proposal and check event
    let proposal_id = SubStreamContract::propose_registry_update(
        env.clone(),
        council_members.get(0).unwrap(),
        merchant.clone(),
        RegistryUpdateType::WhitelistMerchant,
        soroban_sdk::String::from_str(&env, "Test event emission"),
        false,
    );
    
    // Verify RegistryUpdateProposed event was emitted
    let events = env.events().all();
    let proposal_events = events.iter().filter(|event| {
        match event {
            soroban_sdk::xdr::ContractEvent::V0(v0) => {
                let topic = soroban_sdk::Symbol::new(&env, "RegistryUpdateProposed");
                v0.topics.contains(&topic.to_val())
            }
            _ => false,
        }
    }).collect::<Vec<_>>();
    
    assert_eq!(proposal_events.len(), 1);
}

#[test]
fn test_unauthorized_proposer_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let merchant = Address::random(&env);
    let unauthorized_user = Address::random(&env);
    
    // Initialize contract
    SubStreamContract::initialize(env.clone(), admin.clone());
    
    // Try to propose without being council member or admin (should fail)
    let result = env.try_invoke_contract::<u64, (
        &SubStreamContract::propose_registry_update,
        &env,
        &unauthorized_user,
        &merchant,
        &RegistryUpdateType::WhitelistMerchant,
        &soroban_sdk::String::from_str(&env, "Unauthorized proposal"),
        &false,
    );
    assert!(result.is_err());
}

#[test]
fn test_emergency_bypass_requires_admin() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let merchant = Address::random(&env);
    
    // Initialize contract
    SubStreamContract::initialize(env.clone(), admin.clone());
    
    // Set up security council
    let council_members = vec![
        &env,
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
        Address::random(&env),
    ];
    SubStreamContract::initialize_security_council(env.clone(), admin.clone(), council_members.clone());
    
    // Try emergency bypass as council member (should fail - only admin can)
    let result = env.try_invoke_contract::<u64, (
        &SubStreamContract::propose_registry_update,
        &env,
        &council_members.get(0).unwrap(),
        &merchant,
        &RegistryUpdateType::BlacklistMerchant,
        &soroban_sdk::String::from_str(&env, "Emergency attempt"),
        &true, // Emergency bypass
    );
    assert!(result.is_err());
    
    // Should work with admin
    let proposal_id = SubStreamContract::propose_registry_update(
        env.clone(),
        admin.clone(),
        merchant.clone(),
        RegistryUpdateType::BlacklistMerchant,
        soroban_sdk::String::from_str(&env, "Valid emergency"),
        true,
    );
    
    // Verify proposal was created
    let proposal_key = DataKey::RegistryUpdateProposal(proposal_id);
    let proposal: RegistryUpdateProposal = env.storage().persistent().get(&proposal_key).unwrap();
    assert!(proposal.emergency_bypass);
}
