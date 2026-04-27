# SubStream Protocol Invariants Manifest

## Escrow Vault Invariants

This document formally records the invariants governing the SubStream annual escrow vault (Issue #56, #142) to ensure absolute structural protection of retail funds.

### 1. Non-Negative Escrow Balance
**Invariant**: `Escrow_Balance >= 0`
At no point in the lifecycle of any subscription (active, canceled, upgraded, or downgraded) can the total liquidity inside the escrow vault drop below zero. Underflow panics are mathematically impossible.

### 2. Liquidity Conservation
**Invariant**: `Vested + Unvested == Total_Escrowed`
The sum of all individual active sub-balances mathematically equals the total vaulted liquidity held in the smart contract. If a merchant goes bankrupt, users are structurally protected from insolvency because the exact value of their unvested drip is fully collateralized in the contract vault.

### 3. State Transitions & Edge-Case Coverage
The automated formal proof simulates the following state transitions:
- **Trial Boundaries**: Validates mathematical bounds where a trial ends exactly during a continuous pull.
- **Simultaneous Drip & Mutations**: Simulates upgrades, downgrades, and mid-cycle cancellations executing concurrently with automated pulls.
- **Conservation Upon Settlement**: Ensures escrow balances correctly settle prior to any state mutation, mathematically barring leakage.