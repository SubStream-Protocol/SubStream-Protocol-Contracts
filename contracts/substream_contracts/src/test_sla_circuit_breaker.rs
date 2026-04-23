#![cfg(test)]

use soroban_sdk::symbol_short;
use soroban_sdk::testutils::{Ledger, LedgerInfo};
use soroban_sdk::{Address, Env, Vec};

use substream_contracts::{
    SubStreamContract, SubStreamContractClient,
    UptimeOraclePayload, SLAStatus,
    DataKey, SLA_THRESHOLD_BPS, SEVEN_DAYS, UPTIME_ORACLE_NONCE_TTL
};

#[test]
fn test_sla_status_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    
    let contract_id = env.register_contract(None, SubStreamContract);
    let client = SubStreamContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    
    client.initialize(&admin);
    
    // Initially, SLA status should be default values
    let sla_status = client.get_sla_status(&creator);
    assert!(!sla_status.active);
    assert_eq!(sla_status.last_updated, 0);
    assert_eq!(sla_status.cumulative_downtime_minutes, 0);
    assert_eq!(sla_status.current_penalty_period_start, 0);
    assert_eq!(sla_status.total_refund_owed, 0);
}

#[test]
fn test_sla_breach_detection() {
    let env = Env::default();
    env.mock_all_auths();
    
    let contract_id = env.register_contract(None, SubStreamContract);
    let client = SubStreamContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let oracle = Address::generate(&env);
    
    client.initialize(&admin);
    
    // Create uptime oracle payload with SLA breach (99.8% < 99.9% threshold)
    let payload = UptimeOraclePayload {
        creator: creator.clone(),
        uptime_percentage: 9980, // 99.8% in basis points
        downtime_minutes: 1440,    // 24 hours
        period_start: 1000,
        period_end: 2000,
        nonce: 1,
        oracle_signature: Vec::from_array(&env, [1, 2, 3, 4]),
    };
    
    // Mock oracle authentication
    env.mock_auths(&[
        (&oracle, &contract_id, &Symbol::new(&env, "update_sla_status")),
    ]);
    
    // Update SLA status
    client.update_sla_status(&payload);
    
    // Verify SLA breach is detected
    let sla_status = client.get_sla_status(&creator);
    assert!(sla_status.active);
    assert_eq!(sla_status.cumulative_downtime_minutes, 1440);
    assert_eq!(sla_status.current_penalty_period_start, 1000);
    assert!(sla_status.total_refund_owed > 0);
}

#[test]
fn test_sla_recovery() {
    let env = Env::default();
    env.mock_all_auths();
    
    let contract_id = env.register_contract(None, SubStreamContract);
    let client = SubStreamContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let oracle = Address::generate(&env);
    
    client.initialize(&admin);
    
    // First, create an SLA breach
    let breach_payload = UptimeOraclePayload {
        creator: creator.clone(),
        uptime_percentage: 9980, // 99.8% < SLA threshold
        downtime_minutes: 720,     // 12 hours
        period_start: 1000,
        period_end: 2000,
        nonce: 1,
        oracle_signature: Vec::from_array(&env, [1, 2, 3, 4]),
    };
    
    env.mock_auths(&[
        (&oracle, &contract_id, &Symbol::new(&env, "update_sla_status")),
    ]);
    
    client.update_sla_status(&breach_payload);
    
    // Verify breach is active
    let sla_status = client.get_sla_status(&creator);
    assert!(sla_status.active);
    
    // Now report recovery
    let recovery_payload = UptimeOraclePayload {
        creator: creator.clone(),
        uptime_percentage: 9995, // 99.95% > SLA threshold
        downtime_minutes: 0,
        period_start: 2000,
        period_end: 3000,
        nonce: 2,
        oracle_signature: Vec::from_array(&env, [5, 6, 7, 8]),
    };
    
    client.update_sla_status(&recovery_payload);
    
    // Verify SLA is no longer active
    let sla_status = client.get_sla_status(&creator);
    assert!(!sla_status.active);
}

#[test]
fn test_oracle_nonce_validation() {
    let env = Env::default();
    env.mock_all_auths();
    
    let contract_id = env.register_contract(None, SubStreamContract);
    let client = SubStreamContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let oracle = Address::generate(&env);
    
    client.initialize(&admin);
    
    let payload = UptimeOraclePayload {
        creator: creator.clone(),
        uptime_percentage: 9980,
        downtime_minutes: 720,
        period_start: 1000,
        period_end: 2000,
        nonce: 1,
        oracle_signature: Vec::from_array(&env, [1, 2, 3, 4]),
    };
    
    env.mock_auths(&[
        (&oracle, &contract_id, &Symbol::new(&env, "update_sla_status")),
    ]);
    
    // First update should succeed
    client.update_sla_status(&payload);
    
    // Second update with same nonce should fail
    let result = env.try_invoke_contract(
        &contract_id,
        &Symbol::new(&env, "update_sla_status"),
        Vec::from_array(&env, [&payload]),
    );
    
    assert!(result.is_err());
    assert_eq!(
        env.result().unwrap_err().to_string(),
        "Contract(1, \"oracle nonce already used\")"
    );
}

#[test]
fn test_oracle_signature_expiration() {
    let env = Env::default();
    env.mock_all_auths();
    
    let contract_id = env.register_contract(None, SubStreamContract);
    let client = SubStreamContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let oracle = Address::generate(&env);
    
    client.initialize(&admin);
    
    // Set ledger time far in the future to make signature expired
    env.ledger().set_timestamp(1000000);
    
    let payload = UptimeOraclePayload {
        creator: creator.clone(),
        uptime_percentage: 9980,
        downtime_minutes: 720,
        period_start: 1000,
        period_end: 2000, // This is well before current time + TTL
        nonce: 1,
        oracle_signature: Vec::from_array(&env, [1, 2, 3, 4]),
    };
    
    env.mock_auths(&[
        (&oracle, &contract_id, &Symbol::new(&env, "update_sla_status")),
    ]);
    
    // Should fail due to expired signature
    let result = env.try_invoke_contract(
        &contract_id,
        &Symbol::new(&env, "update_sla_status"),
        Vec::from_array(&env, [&payload]),
    );
    
    assert!(result.is_err());
    assert_eq!(
        env.result().unwrap_err().to_string(),
        "Contract(1, \"oracle signature expired\")"
    );
}

#[test]
fn test_emergency_cancellation_after_7_days() {
    let env = Env::default();
    env.mock_all_auths();
    
    let contract_id = env.register_contract(None, SubStreamContract);
    let client = SubStreamContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let subscriber = Address::generate(&env);
    let oracle = Address::generate(&env);
    let token = env.register_stellar_asset_contract(admin.clone());
    
    client.initialize(&admin);
    
    // Create a subscription
    client.subscribe(
        &subscriber,
        &creator,
        &token.address,
        &1000, // 1000 tokens
        &100,   // 100 tokens per second rate
    );
    
    // Create SLA breach with more than 7 days of downtime
    let payload = UptimeOraclePayload {
        creator: creator.clone(),
        uptime_percentage: 9980,
        downtime_minutes: 10800, // 7.5 days = 10800 minutes
        period_start: 1000,
        period_end: 2000,
        nonce: 1,
        oracle_signature: Vec::from_array(&env, [1, 2, 3, 4]),
    };
    
    env.mock_auths(&[
        (&oracle, &contract_id, &Symbol::new(&env, "update_sla_status")),
    ]);
    
    client.update_sla_status(&payload);
    
    // Verify SLA breach is active
    let sla_status = client.get_sla_status(&creator);
    assert!(sla_status.active);
    assert!(sla_status.cumulative_downtime_minutes >= 10080); // 7 days
    
    // Emergency cancellation should succeed
    client.emergency_cancel_due_to_sla(&subscriber, &creator);
    
    // Verify subscription is cancelled
    assert!(!client.is_subscribed(&subscriber, &creator));
}

#[test]
fn test_emergency_cancellation_insufficient_downtime() {
    let env = Env::default();
    env.mock_all_auths();
    
    let contract_id = env.register_contract(None, SubStreamContract);
    let client = SubStreamContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let subscriber = Address::generate(&env);
    let oracle = Address::generate(&env);
    let token = env.register_stellar_asset_contract(admin.clone());
    
    client.initialize(&admin);
    
    // Create a subscription
    client.subscribe(
        &subscriber,
        &creator,
        &token.address,
        &1000,
        &100,
    );
    
    // Create SLA breach with less than 7 days of downtime
    let payload = UptimeOraclePayload {
        creator: creator.clone(),
        uptime_percentage: 9980,
        downtime_minutes: 7200, // 5 days = 7200 minutes
        period_start: 1000,
        period_end: 2000,
        nonce: 1,
        oracle_signature: Vec::from_array(&env, [1, 2, 3, 4]),
    };
    
    env.mock_auths(&[
        (&oracle, &contract_id, &Symbol::new(&env, "update_sla_status")),
    ]);
    
    client.update_sla_status(&payload);
    
    // Emergency cancellation should fail due to insufficient downtime
    let result = env.try_invoke_contract(
        &contract_id,
        &Symbol::new(&env, "emergency_cancel_due_to_sla"),
        Vec::from_array(&env, [&subscriber, &creator]),
    );
    
    assert!(result.is_err());
    assert_eq!(
        env.result().unwrap_err().to_string(),
        "Contract(1, \"SLA emergency cancellation only available after 7+ days of downtime\")"
    );
}

#[test]
fn test_sla_refund_calculation() {
    let env = Env::default();
    env.mock_all_auths();
    
    let contract_id = env.register_contract(None, SubStreamContract);
    let client = SubStreamContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let subscriber = Address::generate(&env);
    let oracle = Address::generate(&env);
    let token = env.register_stellar_asset_contract(admin.clone());
    
    client.initialize(&admin);
    
    // Create a subscription with known rate
    let rate_per_second = 100; // 100 tokens per second
    client.subscribe(
        &subscriber,
        &creator,
        &token.address,
        &10000,
        &rate_per_second,
    );
    
    // Create SLA breach with specific downtime
    let downtime_minutes = 60; // 1 hour
    let payload = UptimeOraclePayload {
        creator: creator.clone(),
        uptime_percentage: 9980,
        downtime_minutes,
        period_start: 1000,
        period_end: 2000,
        nonce: 1,
        oracle_signature: Vec::from_array(&env, [1, 2, 3, 4]),
    };
    
    env.mock_auths(&[
        (&oracle, &contract_id, &Symbol::new(&env, "update_sla_status")),
    ]);
    
    client.update_sla_status(&payload);
    
    // Calculate expected refund: rate * downtime_seconds
    let expected_refund = rate_per_second * (downtime_minutes * 60) as i128;
    
    let sla_status = client.get_sla_status(&creator);
    assert_eq!(sla_status.total_refund_owed, expected_refund);
}

#[test]
fn test_cumulative_downtime_tracking() {
    let env = Env::default();
    env.mock_all_auths();
    
    let contract_id = env.register_contract(None, SubStreamContract);
    let client = SubStreamContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let oracle = Address::generate(&env);
    
    client.initialize(&admin);
    
    // First breach: 2 hours downtime
    let payload1 = UptimeOraclePayload {
        creator: creator.clone(),
        uptime_percentage: 9980,
        downtime_minutes: 120,
        period_start: 1000,
        period_end: 2000,
        nonce: 1,
        oracle_signature: Vec::from_array(&env, [1, 2, 3, 4]),
    };
    
    env.mock_auths(&[
        (&oracle, &contract_id, &Symbol::new(&env, "update_sla_status")),
    ]);
    
    client.update_sla_status(&payload1);
    
    let sla_status = client.get_sla_status(&creator);
    assert_eq!(sla_status.cumulative_downtime_minutes, 120);
    
    // Second breach: 3 hours downtime
    let payload2 = UptimeOraclePayload {
        creator: creator.clone(),
        uptime_percentage: 9970,
        downtime_minutes: 180,
        period_start: 2000,
        period_end: 3000,
        nonce: 2,
        oracle_signature: Vec::from_array(&env, [5, 6, 7, 8]),
    };
    
    client.update_sla_status(&payload2);
    
    // Verify cumulative downtime
    let sla_status = client.get_sla_status(&creator);
    assert_eq!(sla_status.cumulative_downtime_minutes, 300); // 120 + 180
}

#[test]
fn test_sla_breach_event_emission() {
    let env = Env::default();
    env.mock_all_auths();
    
    let contract_id = env.register_contract(None, SubStreamContract);
    let client = SubStreamContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let oracle = Address::generate(&env);
    
    client.initialize(&admin);
    
    let payload = UptimeOraclePayload {
        creator: creator.clone(),
        uptime_percentage: 9980,
        downtime_minutes: 720,
        period_start: 1000,
        period_end: 2000,
        nonce: 1,
        oracle_signature: Vec::from_array(&env, [1, 2, 3, 4]),
    };
    
    env.mock_auths(&[
        (&oracle, &contract_id, &Symbol::new(&env, "update_sla_status")),
    ]);
    
    client.update_sla_status(&payload);
    
    // Check that SLABreached event was emitted
    let events = env.events().all();
    assert!(events.len() >= 1);
    
    // Find the SLABreached event
    let sla_event = events.iter().find(|event| {
        event.topics.len() >= 3 && 
        event.topics.get(0).unwrap().to_symbol() == symbol_short!("SLABreached")
    });
    
    assert!(sla_event.is_some());
}
