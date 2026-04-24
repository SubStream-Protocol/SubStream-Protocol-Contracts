#![cfg(test)]

use crate::{SubStreamContract, SubStreamContractClient, FanContribution};
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

#[test]
fn test_top_fans() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, SubStreamContract);
    let client = SubStreamContractClient::new(&env, &contract_id);

    let creator = Address::generate(&env);
    
    // Generate 60 fans with different contribution amounts
    for i in 1..=60 {
        let fan = Address::generate(&env);
        // We call a function that triggers credit_fan_contribution
        // Since tip is the simplest one to use for this test:
        // pub fn tip(env: Env, user: Address, creator: Address, amount: i128)
        client.tip(&fan, &creator, &(i as i128 * 100));
    }

    let top_fans = client.get_top_fans(&creator);
    
    // Should only have 50 fans
    assert_eq!(top_fans.len(), 50);

    // The top fan should be the 60th one (60 * 100 = 6000)
    assert_eq!(top_fans.get(0).unwrap().total_contributed, 6000);
    
    // The 50th fan should be the 11th one (11 * 100 = 1100)
    assert_eq!(top_fans.get(49).unwrap().total_contributed, 1100);

    // Update an existing fan to become the new #1
    let fan_11 = top_fans.get(49).unwrap().fan;
    client.tip(&fan_11, &creator, &10000); // 1100 + 10000 = 11100
    
    let top_fans_updated = client.get_top_fans(&creator);
    assert_eq!(top_fans_updated.get(0).unwrap().fan, fan_11);
    assert_eq!(top_fans_updated.get(0).unwrap().total_contributed, 11100);
}
