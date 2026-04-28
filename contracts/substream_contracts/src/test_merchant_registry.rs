#![cfg(test)]

use soroban_sdk::{Address, Env, vec};
use crate::{
    SubStreamContract, MerchantStatus, VerificationMethod, KYCCredential, DAOProposal, 
    ProposalType, DAOVote, MerchantWhitelisted, MerchantBlacklisted, KYCCredentialVerified,
    DAOProposalCreated, DAOVoteCast, DAOProposalExecuted, DataKey
};

#[test]
fn test_merchant_registration_with_kyc() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let merchant = Address::random(&env);
    let kyc_issuer = Address::from_string(&soroban_sdk::String::from_str(&env, "GD5DQX2K7Q4D4PE4R6J4Y7Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2"));

    // Create security council members
    let council_member1 = Address::random(&env);
    let council_member2 = Address::random(&env);
    let council_member3 = Address::random(&env);
    let council_member4 = Address::random(&env);
    let council_member5 = Address::random(&env);
    let security_council = vec![&env, council_member1.clone(), council_member2.clone(), council_member3.clone(), council_member4.clone(), council_member5.clone()];

    // Initialize contract with security council and KYC issuer
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council, kyc_issuer.clone());

    // Register merchant with valid KYC
    let kyc_hash = vec![&env; 32u8]; // Mock KYC hash
    SubStreamContract::register_merchant_with_kyc(
        env.clone(),
        merchant.clone(),
        kyc_hash.clone(),
        kyc_issuer.clone(),
    );

    // Verify merchant is registered and verified
    assert!(SubStreamContract::is_merchant_verified(env.clone(), merchant.clone()));

    let merchant_status = SubStreamContract::get_merchant_status(env.clone(), merchant.clone());
    assert!(merchant_status.is_verified);
    assert!(!merchant_status.is_blacklisted);
    assert!(matches!(merchant_status.verification_method, VerificationMethod::SEP12KYC));

    // Verify KYC credential is stored
    let kyc_credential_key = DataKey::KYCCredential(merchant.clone());
    let stored_kyc: KYCCredential = env.storage().persistent().get(&kyc_credential_key).unwrap();
    assert_eq!(stored_kyc.merchant_address, merchant);
    assert_eq!(stored_kyc.issuer, kyc_issuer);
    assert_eq!(stored_kyc.credential_hash, kyc_hash);
    assert!(stored_kyc.is_valid);
}

#[test]
fn test_merchant_registration_unauthorized_issuer() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let merchant = Address::random(&env);
    let kyc_issuer = Address::from_string(&soroban_sdk::String::from_str(&env, "GD5DQX2K7Q4D4PE4R6J4Y7Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2"));
    let unauthorized_issuer = Address::random(&env);

    // Create security council members
    let council_member1 = Address::random(&env);
    let council_member2 = Address::random(&env);
    let council_member3 = Address::random(&env);
    let council_member4 = Address::random(&env);
    let council_member5 = Address::random(&env);
    let security_council = vec![&env, council_member1.clone(), council_member2.clone(), council_member3.clone(), council_member4.clone(), council_member5.clone()];

    // Initialize contract with security council and KYC issuer
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council, kyc_issuer.clone());

    // Try to register merchant with unauthorized KYC issuer
    let kyc_hash = vec![&env; 32u8];
    let result = env.try_invoke_contract::<(), (
        &SubStreamContract::register_merchant_with_kyc,
        &env,
        &merchant,
        &kyc_hash,
        &unauthorized_issuer,
    );

    // Should fail with unauthorized KYC issuer error
    assert!(result.is_err());
    assert!(!SubStreamContract::is_merchant_verified(env.clone(), merchant.clone()));
}

#[test]
fn test_dao_proposal_and_approval() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let merchant = Address::random(&env);
    let proposer = Address::random(&env);
    let kyc_issuer = Address::random(&env);

    // Create security council members
    let council_member1 = Address::random(&env);
    let council_member2 = Address::random(&env);
    let council_member3 = Address::random(&env);
    let council_member4 = Address::random(&env);
    let council_member5 = Address::random(&env);
    let security_council = vec![&env, council_member1.clone(), council_member2.clone(), council_member3.clone(), council_member4.clone(), council_member5.clone()];

    // Initialize contract with security council and KYC issuer
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council, kyc_issuer);
    
    // First register merchant (without KYC for DAO approval test)
    let merchant_status = MerchantStatus {
        is_verified: false,
        is_blacklisted: false,
        verification_method: VerificationMethod::DAOApproval,
        registered_at: env.ledger().timestamp(),
        last_verified: 0,
        dao_approved: false,
    };
    env.storage().persistent().set(&DataKey::MerchantRegistry(merchant.clone()), &merchant_status);
    
    // Create DAO proposal to whitelist merchant
    let proposal_id = SubStreamContract::create_merchant_proposal(
        env.clone(),
        proposer.clone(),
        merchant.clone(),
        ProposalType::WhitelistMerchant,
        soroban_sdk::String::from_str(&env, "Whitelist legitimate merchant"),
    );
    
    // Verify proposal was created
    let proposal_key = DataKey::DAOProposal(proposal_id);
    let proposal: DAOProposal = env.storage().persistent().get(&proposal_key).unwrap();
    assert_eq!(proposal.merchant_address, merchant);
    assert!(matches!(proposal.proposal_type, ProposalType::WhitelistMerchant));
    assert_eq!(proposal.votes_for, 0);
    assert_eq!(proposal.votes_against, 0);
    assert!(!proposal.executed);
    
    // Vote on proposal (admin acts as DAO member)
    SubStreamContract::vote_on_merchant_proposal(env.clone(), admin.clone(), proposal_id, true);
    
    // Check if merchant is now verified
    assert!(SubStreamContract::is_merchant_verified(env.clone(), merchant.clone()));
    
    let updated_status = SubStreamContract::get_merchant_status(env.clone(), merchant.clone());
    assert!(updated_status.is_verified);
    assert!(updated_status.dao_approved);
    assert!(matches!(updated_status.verification_method, VerificationMethod::DAOApproval));
}

#[test]
fn test_merchant_blacklisting() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let merchant = Address::random(&env);
    let kyc_issuer = Address::from_string(&soroban_sdk::String::from_str(&env, "GD5DQX2K7Q4D4PE4R6J4Y7Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2"));

    // Create security council members
    let council_member1 = Address::random(&env);
    let council_member2 = Address::random(&env);
    let council_member3 = Address::random(&env);
    let council_member4 = Address::random(&env);
    let council_member5 = Address::random(&env);
    let security_council = vec![&env, council_member1.clone(), council_member2.clone(), council_member3.clone(), council_member4.clone(), council_member5.clone()];

    // Initialize contract with security council and KYC issuer
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council, kyc_issuer.clone());
    
    // Register merchant with KYC
    let kyc_hash = vec![&env; 32u8];
    SubStreamContract::register_merchant_with_kyc(
        env.clone(),
        merchant.clone(),
        kyc_hash,
        kyc_issuer.clone(),
    );
    
    // Verify merchant is initially verified
    assert!(SubStreamContract::is_merchant_verified(env.clone(), merchant.clone()));
    
    // Blacklist merchant
    SubStreamContract::blacklist_merchant(
        env.clone(),
        admin.clone(),
        merchant.clone(),
        soroban_sdk::String::from_str(&env, "Malicious activity detected"),
    );
    
    // Verify merchant is now blacklisted and not verified
    assert!(!SubStreamContract::is_merchant_verified(env.clone(), merchant.clone()));
    
    let merchant_status = SubStreamContract::get_merchant_status(env.clone(), merchant.clone());
    assert!(merchant_status.is_blacklisted);
    assert!(!merchant_status.is_verified);
}

#[test]
fn test_subscription_to_unverified_merchant_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let subscriber = Address::random(&env);
    let unverified_merchant = Address::random(&env);
    let token = Address::random(&env);
    let kyc_issuer = Address::random(&env);

    // Create security council members
    let council_member1 = Address::random(&env);
    let council_member2 = Address::random(&env);
    let council_member3 = Address::random(&env);
    let council_member4 = Address::random(&env);
    let council_member5 = Address::random(&env);
    let security_council = vec![&env, council_member1.clone(), council_member2.clone(), council_member3.clone(), council_member4.clone(), council_member5.clone()];

    // Initialize contract with security council and KYC issuer
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council, kyc_issuer);
    
    // Try to subscribe to unverified merchant
    let result = env.try_invoke_contract::<(), (
        &SubStreamContract::subscribe,
        &env,
        &subscriber,
        &unverified_merchant,
        &token,
        1000i128,
        1i128,
        None::<Address>,
    );
    
    // Should fail with "creator is not a verified merchant" error
    assert!(result.is_err());
}

#[test]
fn test_subscription_to_blacklisted_merchant_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let subscriber = Address::random(&env);
    let merchant = Address::random(&env);
    let kyc_issuer = Address::from_string(&soroban_sdk::String::from_str(&env, "GD5DQX2K7Q4D4PE4R6J4Y7Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2"));
    let token = Address::random(&env);

    // Create security council members
    let council_member1 = Address::random(&env);
    let council_member2 = Address::random(&env);
    let council_member3 = Address::random(&env);
    let council_member4 = Address::random(&env);
    let council_member5 = Address::random(&env);
    let security_council = vec![&env, council_member1.clone(), council_member2.clone(), council_member3.clone(), council_member4.clone(), council_member5.clone()];

    // Initialize contract with security council and KYC issuer
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council, kyc_issuer.clone());
    
    // Register merchant with KYC
    let kyc_hash = vec![&env; 32u8];
    SubStreamContract::register_merchant_with_kyc(
        env.clone(),
        merchant.clone(),
        kyc_hash,
        kyc_issuer.clone(),
    );
    
    // Blacklist merchant
    SubStreamContract::blacklist_merchant(
        env.clone(),
        admin.clone(),
        merchant.clone(),
        soroban_sdk::String::from_str(&env, "Suspicious activity"),
    );
    
    // Try to subscribe to blacklisted merchant
    let result = env.try_invoke_contract::<(), (
        &SubStreamContract::subscribe,
        &env,
        &subscriber,
        &merchant,
        &token,
        1000i128,
        1i128,
        None::<Address>,
    );
    
    // Should fail with "creator is not a verified merchant" error
    assert!(result.is_err());
}

#[test]
fn test_blacklisted_merchant_cannot_collect_funds() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let subscriber = Address::random(&env);
    let merchant = Address::random(&env);
    let kyc_issuer = Address::from_string(&soroban_sdk::String::from_str(&env, "GD5DQX2K7Q4D4PE4R6J4Y7Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2"));
    let token = Address::random(&env);

    // Create security council members
    let council_member1 = Address::random(&env);
    let council_member2 = Address::random(&env);
    let council_member3 = Address::random(&env);
    let council_member4 = Address::random(&env);
    let council_member5 = Address::random(&env);
    let security_council = vec![&env, council_member1.clone(), council_member2.clone(), council_member3.clone(), council_member4.clone(), council_member5.clone()];

    // Initialize contract with security council and KYC issuer
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council, kyc_issuer.clone());
    
    // Register merchant with KYC
    let kyc_hash = vec![&env; 32u8];
    SubStreamContract::register_merchant_with_kyc(
        env.clone(),
        merchant.clone(),
        kyc_hash,
        kyc_issuer.clone(),
    );
    
    // Create subscription
    SubStreamContract::subscribe(
        env.clone(),
        subscriber.clone(),
        merchant.clone(),
        token.clone(),
        1000i128,
        1i128,
        None::<Address>,
    );
    
    // Blacklist merchant after subscription
    SubStreamContract::blacklist_merchant(
        env.clone(),
        admin.clone(),
        merchant.clone(),
        soroban_sdk::String::from_str(&env, "Policy violation"),
    );
    
    // Try to collect funds (should return 0 for blacklisted merchant)
    let collected = SubStreamContract::collect(env.clone(), subscriber.clone(), merchant.clone());
    assert_eq!(collected, 0);
    
    // Verify subscriber funds are protected
    let subscription_key = DataKey::Subscription(subscriber.clone(), merchant.clone());
    let subscription = env.storage().persistent().get::<crate::Subscription>(&subscription_key).unwrap();
    assert!(subscription.balance > 0); // Funds should still be there
}

#[test]
fn test_merchant_cannot_register_twice() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let merchant = Address::random(&env);
    let kyc_issuer = Address::from_string(&soroban_sdk::String::from_str(&env, "GD5DQX2K7Q4D4PE4R6J4Y7Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2"));

    // Create security council members
    let council_member1 = Address::random(&env);
    let council_member2 = Address::random(&env);
    let council_member3 = Address::random(&env);
    let council_member4 = Address::random(&env);
    let council_member5 = Address::random(&env);
    let security_council = vec![&env, council_member1.clone(), council_member2.clone(), council_member3.clone(), council_member4.clone(), council_member5.clone()];

    // Initialize contract with security council and KYC issuer
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council, kyc_issuer.clone());
    
    // Register merchant with KYC
    let kyc_hash = vec![&env; 32u8];
    SubStreamContract::register_merchant_with_kyc(
        env.clone(),
        merchant.clone(),
        kyc_hash.clone(),
        kyc_issuer.clone(),
    );
    
    // Try to register again
    let result = env.try_invoke_contract::<(), (
        &SubStreamContract::register_merchant_with_kyc,
        &env,
        &merchant,
        &kyc_hash,
        &kyc_issuer,
    );
    
    // Should fail with "merchant already registered" error
    assert!(result.is_err());
}

#[test]
fn test_dao_proposal_voting_mechanism() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let merchant = Address::random(&env);
    let proposer = Address::random(&env);
    let voter1 = Address::random(&env);
    let voter2 = Address::random(&env);
    let kyc_issuer = Address::random(&env);

    // Create security council members
    let council_member1 = Address::random(&env);
    let council_member2 = Address::random(&env);
    let council_member3 = Address::random(&env);
    let council_member4 = Address::random(&env);
    let council_member5 = Address::random(&env);
    let security_council = vec![&env, council_member1.clone(), council_member2.clone(), council_member3.clone(), council_member4.clone(), council_member5.clone()];

    // Initialize contract with security council and KYC issuer
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council, kyc_issuer);
    
    // Set up merchant for proposal
    let merchant_status = MerchantStatus {
        is_verified: false,
        is_blacklisted: false,
        verification_method: VerificationMethod::DAOApproval,
        registered_at: env.ledger().timestamp(),
        last_verified: 0,
        dao_approved: false,
    };
    env.storage().persistent().set(&DataKey::MerchantRegistry(merchant.clone()), &merchant_status);
    
    // Create DAO proposal
    let proposal_id = SubStreamContract::create_merchant_proposal(
        env.clone(),
        proposer.clone(),
        merchant.clone(),
        ProposalType::WhitelistMerchant,
        soroban_sdk::String::from_str(&env, "Whitelist for review"),
    );
    
    // Multiple votes (simulate DAO voting)
    SubStreamContract::vote_on_merchant_proposal(env.clone(), admin.clone(), proposal_id, true);
    SubStreamContract::vote_on_merchant_proposal(env.clone(), voter1.clone(), proposal_id, true);
    SubStreamContract::vote_on_merchant_proposal(env.clone(), voter2.clone(), proposal_id, true);
    
    // Verify proposal was executed and merchant is verified
    let proposal_key = DataKey::DAOProposal(proposal_id);
    let proposal: DAOProposal = env.storage().persistent().get(&proposal_key).unwrap();
    assert!(proposal.executed);
    assert_eq!(proposal.votes_for, 3); // Threshold reached
    assert!(SubStreamContract::is_merchant_verified(env.clone(), merchant.clone()));
}

#[test]
fn test_events_emission() {
    let env = Env::default();
    let contract_id = env.register_contract(None, SubStreamContract);
    let admin = Address::random(&env);
    let merchant = Address::random(&env);
    let kyc_issuer = Address::from_string(&soroban_sdk::String::from_str(&env, "GD5DQX2K7Q4D4PE4R6J4Y7Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2Q2"));

    // Create security council members
    let council_member1 = Address::random(&env);
    let council_member2 = Address::random(&env);
    let council_member3 = Address::random(&env);
    let council_member4 = Address::random(&env);
    let council_member5 = Address::random(&env);
    let security_council = vec![&env, council_member1.clone(), council_member2.clone(), council_member3.clone(), council_member4.clone(), council_member5.clone()];

    // Initialize contract with security council and KYC issuer
    SubStreamContract::initialize(env.clone(), admin.clone(), security_council, kyc_issuer.clone());
    
    // Register merchant and check events
    let kyc_hash = vec![&env; 32u8];
    SubStreamContract::register_merchant_with_kyc(
        env.clone(),
        merchant.clone(),
        kyc_hash,
        kyc_issuer.clone(),
    );
    
    // Verify KYC credential verified event
    let kyc_events = env.events().all().filter(|event| {
        match event {
            soroban_sdk::xdr::ContractEvent::V0(v0) => {
                let topic = soroban_sdk::Symbol::new(&env, "KYCCredentialVerified");
                v0.topics.contains(&topic.to_val())
            }
            _ => false,
        }
    });
    assert_eq!(kyc_events.len(), 1);
    
    // Verify merchant whitelisted event
    let whitelist_events = env.events().all().filter(|event| {
        match event {
            soroban_sdk::xdr::ContractEvent::V0(v0) => {
                let topic = soroban_sdk::Symbol::new(&env, "MerchantWhitelisted");
                v0.topics.contains(&topic.to_val())
            }
            _ => false,
        }
    });
    assert_eq!(whitelist_events.len(), 1);
}
