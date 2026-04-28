# Protocol Fee Implementation Status

## Overview

The SubStream Protocol now supports **dynamic, DAO-governed protocol fee parameter updates** with rigorous access control. This implementation provides:

- **DAO-controlled fee changes** through a 5-member Security Council
- **3-of-5 multi-sig consensus** required for fee updates
- **7-day timelock** for fee increases (immediate execution for decreases)
- **Maximum fee cap** of 500 bps (5%)
- **Security Council veto power** - any single member can veto a proposal
- **Transparent event emissions** for on-chain governance tracking
- **Default fee** of 200 bps (2%)

## Implementation Status

### ✅ Completed Components

1. **Protocol Fee Configuration**
   - `ProtocolFeeConfig` struct with current fee, updater address, and timestamp
   - `initialize_protocol_fee()` - admin-only initialization
   - `get_protocol_fee_config()` - read-only query

2. **DAO-Governed Fee Update Process**
   - `propose_protocol_fee_update()` - Security Council or admin can propose
   - `vote_protocol_fee_update()` - Security Council members vote (3-of-5 consensus)
   - `execute_protocol_fee_update()` - Anyone can execute after consensus + timelock
   - `security_council_veto_fee()` - Any Security Council member can veto

3. **Access Control Mechanisms**
   - `is_security_council_member()` - verifies Security Council membership
   - `is_authorized_proposer()` - Security Council members or admin can propose
   - `is_authorized_dao_member()` - alias for Security Council membership
   - `is_authorized_voter()` - Security Council members can vote

4. **Timelock Enforcement**
   - 7-day timelock for fee increases (PROTOCOL_FEE_TIMELOCK_DURATION)
   - Immediate execution for fee decreases
   - Timelock bypass not available for protocol fees (unlike registry updates)

5. **Security Council Veto**
   - Any single Security Council member can veto a pending proposal
   - Vetoed proposals cannot be executed
   - Veto events are emitted for transparency

6. **Events for Transparency**
   - `ProtocolFeeUpdateScheduled` - emitted when proposal is created
   - `ProtocolFeeUpdateExecuted` - emitted when fee is updated
   - `ProtocolFeeUpdateCanceled` - emitted when proposal is canceled
   - `SecurityCouncilVetoedFee` - emitted when proposal is vetoed

7. **Comprehensive Test Suite**
   - Protocol fee initialization
   - Fee increase and decrease proposals
   - Maximum fee enforcement (500 bps)
   - Timelock enforcement for increases
   - Immediate execution for decreases
   - Multi-sig consensus (3-of-5)
   - Security Council veto power
   - Unauthorized proposer rejection
   - Event emission verification
   - Distribution math precision

### ✅ Completed Helper Functions

1. **Authorization Functions**
   - `is_security_council_member()` - checks if address is active Security Council member
   - `is_authorized_proposer()` - checks if address can propose (Security Council or admin)
   - `is_authorized_voter()` - checks if address can vote (Security Council)
   - `is_authorized_dao_member()` - alias for Security Council membership

2. **Proposal ID Generation**
   - `generate_protocol_fee_proposal_id()` - generates unique proposal IDs
   - `generate_registry_proposal_id()` - generates unique registry proposal IDs
   - `generate_proposal_id()` - generates unique DAO proposal IDs

3. **Proposal Execution**
   - `execute_protocol_fee_update_internal()` - internal function to execute fee updates
   - `execute_registry_update()` - internal function to execute registry updates
   - `execute_merchant_proposal()` - internal function to execute merchant proposals

## Core Functions

### Public Interface

```rust
// Initialize protocol fee (admin only)
pub fn initialize_protocol_fee(env: Env, admin: Address)

// Get current protocol fee configuration
pub fn get_protocol_fee_config(env: Env) -> ProtocolFeeConfig

// Propose a protocol fee update (Security Council or admin)
pub fn propose_protocol_fee_update(
    env: Env,
    proposer: Address,
    new_fee_bps: u32,
    reason: soroban_sdk::String,
) -> u64

// Vote on a protocol fee proposal (Security Council only)
pub fn vote_protocol_fee_update(env: Env, voter: Address, proposal_id: u64)

// Execute a protocol fee proposal (anyone after consensus + timelock)
pub fn execute_protocol_fee_update(env: Env, executor: Address, proposal_id: u64)

// Veto a protocol fee proposal (Security Council only)
pub fn security_council_veto_fee(
    env: Env,
    council_member: Address,
    proposal_id: u64,
    veto_reason: soroban_sdk::String,
)

// Get a protocol fee proposal by ID
pub fn get_protocol_fee_proposal(env: Env, proposal_id: u64) -> ProtocolFeeUpdateProposal
```

### Data Structures

```rust
pub struct ProtocolFeeConfig {
    pub current_fee_bps: u32,
    pub updated_by: Address,
    pub last_updated: u64,
}

pub struct ProtocolFeeUpdateProposal {
    pub proposal_id: u64,
    pub new_fee_bps: u32,
    pub old_fee_bps: u32,
    pub proposed_by: Address,
    pub proposed_at: u64,
    pub executable_at: u64,
    pub is_fee_increase: bool,
    pub votes_for: soroban_sdk::Vec<Address>,
    pub executed: bool,
    pub canceled: bool,
    pub reason: soroban_sdk::String,
}

pub struct SecurityCouncilVetoFee {
    pub council_member: Address,
    pub proposal_id: u64,
    pub veto_reason: soroban_sdk::String,
    pub vetoed_at: u64,
}
```

### Events

```rust
pub struct ProtocolFeeUpdateScheduled { ... }
pub struct ProtocolFeeUpdateExecuted { ... }
pub struct ProtocolFeeUpdateCanceled { ... }
pub struct SecurityCouncilVetoedFee { ... }
```

## Design Decisions

### 1. Security Council as DAO

The Security Council (5 members) serves as the DAO for protocol fee governance. This provides:
- **Decentralized control** - no single point of failure
- **Multi-sig consensus** - 3-of-5 required for changes
- **Veto power** - any member can block malicious proposals
- **Quick response** - can act faster than token-based governance

### 2. Timelock for Fee Increases Only

Fee increases require a 7-day timelock to:
- Give the community time to react
- Allow Security Council members to veto if needed
- Prevent rapid fee hikes

Fee decreases execute immediately to:
- Allow quick response to market conditions
- Reduce user costs when appropriate

### 3. Maximum Fee Cap

The 500 bps (5%) maximum fee:
- Prevents excessive protocol fees
- Ensures protocol remains competitive
- Can be changed by future DAO proposals if needed

### 4. No Emergency Bypass for Protocol Fees

Unlike registry updates, protocol fee updates do not have an emergency bypass. This:
- Ensures timelock is always respected for increases
- Prevents abuse of emergency powers
- Maintains community trust

## Integration Steps

### 1. Contract Initialization

```rust
// Initialize contract with Security Council
SubStreamContract::initialize(
    env,
    admin,
    security_council, // 5 addresses
    kyc_issuer,
);

// Initialize protocol fee (separate step)
SubStreamContract::initialize_protocol_fee(env, admin);
```

### 2. Proposing a Fee Change

```rust
// Security Council member proposes fee increase
let proposal_id = SubStreamContract::propose_protocol_fee_update(
    env,
    council_member,
    300, // new fee in bps
    String::from_str(&env, "Increase to fund development"),
);
```

### 3. Voting and Execution

```rust
// Security Council members vote (need 3-of-5)
for voter in &security_council[0..3] {
    SubStreamContract::vote_protocol_fee_update(env, voter.clone(), proposal_id);
}

// For fee increases, wait 7 days then execute
env.ledger().set_timestamp(proposal.executable_at);
SubStreamContract::execute_protocol_fee_update(env, anyone, proposal_id);

// For fee decreases, execution is automatic after 3 votes
```

### 4. Vetoing a Proposal

```rust
// Any Security Council member can veto
SubStreamContract::security_council_veto_fee(
    env,
    council_member,
    proposal_id,
    String::from_str(&env, "Too high, will hurt adoption"),
);
```

## Security Considerations

### Access Control

- **Proposal**: Only Security Council members or admin can propose
- **Voting**: Only Security Council members can vote
- **Execution**: Anyone can execute after consensus + timelock
- **Veto**: Any Security Council member can veto

### Timelock Protection

- Fee increases require 7-day timelock
- Timelock cannot be bypassed
- Prevents rapid fee hikes

### Multi-sig Consensus

- 3-of-5 Security Council members must vote
- Prevents unilateral control
- No single member can execute changes

### Veto Power

- Any single Security Council member can veto
- Provides emergency stop mechanism
- Prevents malicious proposals

## Testing Strategy

### Unit Tests

- ✅ Protocol fee initialization
- ✅ Fee increase and decrease proposals
- ✅ Maximum fee enforcement (500 bps)
- ✅ No-change fee rejection
- ✅ Timelock enforcement for increases
- ✅ Immediate execution for decreases
- ✅ Multi-sig consensus (3-of-5)
- ✅ Security Council veto power
- ✅ Unauthorized proposer rejection
- ✅ Event emission verification
- ✅ Multiple proposals
- ✅ Distribution math precision

### Integration Tests

- Run full governance flow with Security Council
- Verify timelock enforcement in realistic scenarios
- Test veto mechanism during voting
- Verify event emissions across governance lifecycle

## Acceptance Criteria

- [x] Protocol fee can be initialized with default value (200 bps)
- [x] Only Security Council members or admin can propose fee changes
- [x] Fee increases require 3-of-5 Security Council votes
- [x] Fee increases have 7-day timelock
- [x] Fee decreases execute immediately after consensus
- [x] Maximum fee capped at 500 bps (5%)
- [x] Any Security Council member can veto proposals
- [x] All governance actions emit transparent events
- [x] Comprehensive test suite passes
- [x] Access control is rigorous and well-documented

## Future Enhancements

1. **Token-based Governance** - Transition to token-based voting for broader community participation
2. **Proposal Cancellation** - Allow proposers to cancel their own proposals
3. **Fee Distribution Integration** - Automatically distribute collected fees to DAO treasury
4. **Governance Dashboard** - Frontend interface for tracking proposals and votes
5. **Proposal History** - Maintain historical record of all fee changes

## References

- [Governance Documentation](./docs/GOVERNANCE.md)
- [Security Documentation](./SECURITY.md)
- [Test File](./contracts/substream_contracts/src/test_protocol_fee.rs)
- [Timelock Governance Tests](./contracts/substream_contracts/src/test_timelock_governance.rs)
