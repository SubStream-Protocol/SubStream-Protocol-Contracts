#![cfg(test)]

use super::*;
use soroban_sdk::{vec, Address, Env};

#[test]
fn test_rotate_family_vault_signer_updates_authorized_signers() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(SubStreamContract, &());
    let client = SubStreamContractClient::new(&env, &contract_id);

    let vault_id = Address::generate(&env);
    let owner = Address::generate(&env);
    let signer_a = Address::generate(&env);
    let signer_b = Address::generate(&env);
    let new_signer = Address::generate(&env);
    let token = Address::generate(&env);

    let signers = vec![&env, owner.clone(), signer_a.clone(), signer_b.clone()];
    client.create_family_vault(&vault_id, &signers, &2u32, &1000i128, &token);

    client.rotate_family_vault_signer(&vault_id, &signer_a, &new_signer);

    let vault_config: FamilyVaultConfig = env
        .storage()
        .persistent()
        .get(&DataKey::FamilyVault(vault_id.clone()))
        .expect("vault should exist");

    assert_eq!(vault_config.signers.len(), 3);
    assert_eq!(vault_config.signers.get(0).unwrap(), &owner);
    assert_eq!(vault_config.signers.get(1).unwrap(), &new_signer);
    assert_eq!(vault_config.signers.get(2).unwrap(), &signer_b);
}

#[test]
#[should_panic(expected = "new signer already authorized")]
fn test_rotate_family_vault_signer_rejects_duplicate_new_signer() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(SubStreamContract, &());
    let client = SubStreamContractClient::new(&env, &contract_id);

    let vault_id = Address::generate(&env);
    let owner = Address::generate(&env);
    let signer_a = Address::generate(&env);
    let signer_b = Address::generate(&env);
    let token = Address::generate(&env);

    let signers = vec![&env, owner.clone(), signer_a.clone(), signer_b.clone()];
    client.create_family_vault(&vault_id, &signers, &2u32, &1000i128, &token);

    client.rotate_family_vault_signer(&vault_id, &signer_a, &signer_b);
}

#[test]
#[should_panic(expected = "signer not found")]
fn test_rotate_family_vault_signer_requires_existing_signer() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(SubStreamContract, &());
    let client = SubStreamContractClient::new(&env, &contract_id);

    let vault_id = Address::generate(&env);
    let owner = Address::generate(&env);
    let signer_a = Address::generate(&env);
    let signer_b = Address::generate(&env);
    let invalid_signer = Address::generate(&env);
    let token = Address::generate(&env);

    let signers = vec![&env, owner.clone(), signer_a.clone(), signer_b.clone()];
    client.create_family_vault(&vault_id, &signers, &2u32, &1000i128, &token);

    client.rotate_family_vault_signer(&vault_id, &invalid_signer, &Address::generate(&env));
}
