# SubStream Protocol — Disaster Recovery Runbook

> **BREAK GLASS IN CASE OF EMERGENCY**
>
> This document is the authoritative emergency response guide for the SubStream
> Security Council and DAO multi-sig key-holders. Read it **before** a crisis
> occurs. Under stress, follow each step exactly as written.

---

## Table of Contents

1. [Severity Classification](#1-severity-classification)
2. [Emergency Contacts & Multi-Sig Roster](#2-emergency-contacts--multi-sig-roster)
3. [Scenario A — Active Exploit / Fund Drain](#3-scenario-a--active-exploit--fund-drain)
4. [Scenario B — Oracle Manipulation / Bad Price Feed](#4-scenario-b--oracle-manipulation--bad-price-feed)
5. [Scenario C — Governance Attack / Malicious Proposal](#5-scenario-c--governance-attack--malicious-proposal)
6. [Scenario D — WASM Upgrade Gone Wrong](#6-scenario-d--wasm-upgrade-gone-wrong)
7. [Scenario E — Stellar Network Outage](#7-scenario-e--stellar-network-outage)
8. [Multi-Sig Coordination Protocol](#8-multi-sig-coordination-protocol)
9. [V1 → V2 State Migration Procedure](#9-v1--v2-state-migration-procedure)
10. [Post-Incident Checklist](#10-post-incident-checklist)

---

## 1. Severity Classification

| Level | Description | Response Time | Action |
|-------|-------------|---------------|--------|
| **P0** | Active fund drain, contract exploit in progress | Immediate | Emergency pause + all hands |
| **P1** | Vulnerability confirmed, not yet exploited | < 1 hour | Coordinate pause + patch |
| **P2** | Degraded service, no fund risk | < 4 hours | Monitor + scheduled fix |
| **P3** | Minor bug, no user impact | < 24 hours | Normal PR process |

---

## 2. Emergency Contacts & Multi-Sig Roster

The Security Council consists of **5 members**. A minimum of **3 signatures**
are required to authorize any emergency action.

> **Do not store private keys or IP addresses in this document.**
> Key-holders must be contacted via the pre-agreed out-of-band channel
> (Signal group: "SubStream Security Council").

| Role | Identifier | Backup Contact |
|------|-----------|----------------|
| Council Member 1 | Stored in hardware wallet — contact via Signal | Backup: encrypted email |
| Council Member 2 | Stored in hardware wallet — contact via Signal | Backup: encrypted email |
| Council Member 3 | Stored in hardware wallet — contact via Signal | Backup: encrypted email |
| Council Member 4 | Stored in hardware wallet — contact via Signal | Backup: encrypted email |
| Council Member 5 | Stored in hardware wallet — contact via Signal | Backup: encrypted email |

**Contract ID (Testnet):** `CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L`

---

## 3. Scenario A — Active Exploit / Fund Drain

### Detection Signals
- Unusual token transfer volume from contract address
- Cancel velocity circuit breaker fires unexpectedly
- Community reports of unexpected balance changes

### Step 1 — Confirm the Exploit

```bash
# Check recent contract events on Stellar Testnet
stellar contract events \
  --id CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L \
  --network testnet \
  --start-ledger <LAST_KNOWN_GOOD_LEDGER>
```

### Step 2 — Trigger Emergency Pause (requires 3-of-5 multi-sig)

Each Security Council member runs the following independently:

```bash
# Member signs the emergency pause proposal
stellar contract invoke \
  --id CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L \
  --network testnet \
  --source <COUNCIL_MEMBER_SECRET_KEY_ALIAS> \
  -- propose_registry_update \
  --proposer <COUNCIL_MEMBER_ADDRESS> \
  --merchant <EXPLOITED_MERCHANT_ADDRESS> \
  --update_type BlacklistMerchant \
  --description "Emergency: exploit detected, pausing merchant" \
  --emergency_bypass true
```

```bash
# Each additional member votes to reach 3-of-5 threshold
stellar contract invoke \
  --id CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L \
  --network testnet \
  --source <COUNCIL_MEMBER_SECRET_KEY_ALIAS> \
  -- vote_registry_update \
  --voter <COUNCIL_MEMBER_ADDRESS> \
  --proposal_id <PROPOSAL_ID>
```

### Step 3 — Verify Pause is Active

```bash
stellar contract invoke \
  --id CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L \
  --network testnet \
  --source <READ_ONLY_KEY_ALIAS> \
  -- is_protocol_soft_paused
```

Expected output: `true`

### Step 4 — Snapshot Current State

```bash
# Dump all active subscriptions for solvency audit
stellar contract invoke \
  --id CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L \
  --network testnet \
  --source <READ_ONLY_KEY_ALIAS> \
  -- get_active_subscriptions_paginated \
  --page_size 100 \
  --cursor 0 > state_snapshot_$(date +%s).json
```

### Step 5 — Notify Community

Post to official Discord and Twitter within 15 minutes:

> "We have detected an anomaly and have paused the protocol as a precaution.
> User funds are safe. We are investigating and will provide an update within
> [X] hours. No action is required from users."

---

## 4. Scenario B — Oracle Manipulation / Bad Price Feed

### Detection Signals
- SLA oracle reports implausible uptime values (e.g., 100% for all creators simultaneously)
- Abnormal refund payouts triggered by `report_sla_breach`
- Uptime oracle nonce reuse attempts

### Step 1 — Identify Affected Subscriptions

```bash
# Query SLA status for affected creator
stellar contract invoke \
  --id CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L \
  --network testnet \
  --source <READ_ONLY_KEY_ALIAS> \
  -- get_sla_status \
  --creator <AFFECTED_CREATOR_ADDRESS>
```

### Step 2 — Blacklist Compromised Oracle Nonces

The `UPTIME_ORACLE_NONCE_TTL` is 24 hours. Nonces expire automatically.
If the oracle key is compromised, rotate it via a DAO proposal:

```bash
stellar contract invoke \
  --id CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L \
  --network testnet \
  --source <ADMIN_KEY_ALIAS> \
  -- propose_registry_update \
  --proposer <ADMIN_ADDRESS> \
  --merchant <ORACLE_ADDRESS> \
  --update_type BlacklistMerchant \
  --description "Oracle key compromised — rotating" \
  --emergency_bypass true
```

### Step 3 — Reset Circuit Breaker if Triggered

```bash
stellar contract invoke \
  --id CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L \
  --network testnet \
  --source <ADMIN_KEY_ALIAS> \
  -- reset_cancel_velocity_circuit_breaker \
  --admin <ADMIN_ADDRESS>
```

---

## 5. Scenario C — Governance Attack / Malicious Proposal

### Detection Signals
- A DAO proposal targets a legitimate merchant for blacklisting
- Votes accumulate suspiciously fast (Sybil attack)
- A registry update proposal has `emergency_bypass: true` without a declared emergency

### Step 1 — Veto the Proposal Immediately

Any single Security Council member can veto:

```bash
stellar contract invoke \
  --id CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L \
  --network testnet \
  --source <COUNCIL_MEMBER_SECRET_KEY_ALIAS> \
  -- security_council_veto \
  --council_member <COUNCIL_MEMBER_ADDRESS> \
  --proposal_id <MALICIOUS_PROPOSAL_ID> \
  --veto_reason "Suspected governance attack — proposal vetoed pending investigation"
```

### Step 2 — Verify Proposal is Canceled

```bash
# Confirm proposal.canceled == true in storage
stellar contract invoke \
  --id CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L \
  --network testnet \
  --source <READ_ONLY_KEY_ALIAS> \
  -- get_registry_proposal \
  --proposal_id <MALICIOUS_PROPOSAL_ID>
```

### Step 3 — Investigate Voter Addresses

Export the `DAOVoteCast` events and cross-reference voter addresses with known
Sybil patterns. Report findings to the DAO forum within 24 hours.

---

## 6. Scenario D — WASM Upgrade Gone Wrong

### Detection Signals
- Tests pass locally but contract panics on-chain after upgrade
- Unexpected state deserialization errors
- `cargo test` passes but `stellar contract invoke` returns errors

### Step 1 — Do NOT Upgrade Mainnet Until Testnet is Stable

All upgrades must pass the full test suite on Testnet for a minimum of 48 hours
before Mainnet deployment.

### Step 2 — Build the Rollback WASM

```bash
# Check out the last known-good tag
git checkout <LAST_GOOD_TAG>

# Build the rollback WASM
cargo build --target wasm32-unknown-unknown --release

# Compress for deployment
cd contracts/substream_contracts
make build-compressed
```

### Step 3 — Deploy Rollback via Multi-Sig

The Stellar `stellar contract upload` + `stellar contract upgrade` flow requires
the contract admin key. This must be a multi-sig account.

```bash
# Upload rollback WASM
stellar contract upload \
  --network testnet \
  --source <ADMIN_KEY_ALIAS> \
  --wasm target/compressed/substream_contracts.wasm

# Upgrade contract to rollback WASM hash
stellar contract upgrade \
  --id CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L \
  --network testnet \
  --source <ADMIN_KEY_ALIAS> \
  --wasm-hash <ROLLBACK_WASM_HASH>
```

### Step 4 — Verify State Integrity After Rollback

```bash
# Run the full integration test suite against the live contract
cargo test -- --nocapture 2>&1 | tee rollback_verification_$(date +%s).log
```

All tests must pass before announcing the rollback is complete.

---

## 7. Scenario E — Stellar Network Outage

### Detection Signals
- Stellar Horizon API returns 503 errors
- Ledger sequence stops advancing
- `stellar network status --network testnet` shows degraded state

### Immediate Actions

1. **Do nothing destructive.** The contract state is preserved on-chain.
2. Monitor the [Stellar Status Page](https://status.stellar.org) for updates.
3. Notify users via Discord that the network is experiencing issues — this is
   outside the protocol's control.
4. If the outage exceeds 24 hours, activate vacation mode for all merchants
   once the network recovers to prevent spurious billing:

```bash
stellar contract invoke \
  --id CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L \
  --network testnet \
  --source <MERCHANT_KEY_ALIAS> \
  -- activate_vacation_mode \
  --merchant <MERCHANT_ADDRESS>
```

5. Deactivate vacation mode once the network is confirmed stable.

---

## 8. Multi-Sig Coordination Protocol

### Communication Channels (in priority order)

1. **Signal group** — "SubStream Security Council" (primary, end-to-end encrypted)
2. **Encrypted email** — PGP keys exchanged offline during onboarding
3. **Emergency phone tree** — Numbers stored in each member's hardware wallet notes

### Signing Ceremony

1. Incident commander posts the exact CLI command to the Signal group.
2. Each member independently verifies the command against this runbook.
3. Members sign in sequence, posting their transaction hash to the group.
4. Incident commander confirms 3-of-5 threshold is reached on-chain.
5. All actions are logged in the post-incident report.

### Timelock Override

Emergency proposals with `emergency_bypass: true` execute immediately upon
reaching the 3-of-5 threshold. Use this **only** for P0 incidents. All
emergency bypass actions must be justified in the post-incident report.

---

## 9. V1 → V2 State Migration Procedure

Use this procedure when a critical bug requires deploying a new contract
(V2) and migrating trapped user state from V1.

### Phase 1 — Freeze V1

```bash
# Pause V1 via circuit breaker (prevents new state changes)
stellar contract invoke \
  --id <V1_CONTRACT_ID> \
  --network testnet \
  --source <ADMIN_KEY_ALIAS> \
  -- reset_cancel_velocity_circuit_breaker \
  --admin <ADMIN_ADDRESS>
```

### Phase 2 — Export V1 State

```bash
# Export all active subscriptions
stellar contract invoke \
  --id <V1_CONTRACT_ID> \
  --network testnet \
  --source <READ_ONLY_KEY_ALIAS> \
  -- get_active_subscriptions_paginated \
  --page_size 100 \
  --cursor 0 > v1_subscriptions.json

# Export merchant registry
stellar contract invoke \
  --id <V1_CONTRACT_ID> \
  --network testnet \
  --source <READ_ONLY_KEY_ALIAS> \
  -- get_merchant_registry > v1_merchants.json
```

### Phase 3 — Deploy V2

```bash
# Build V2
git checkout main
cargo build --target wasm32-unknown-unknown --release
cd contracts/substream_contracts && make build-compressed

# Deploy V2
stellar contract deploy \
  --network testnet \
  --source <ADMIN_KEY_ALIAS> \
  --wasm target/compressed/substream_contracts.wasm \
  -- --admin <ADMIN_ADDRESS>
```

### Phase 4 — Import State into V2

Write a migration script using the exported JSON files. The script must:

1. Re-register all merchants via `register_merchant_with_kyc` or DAO approval.
2. Re-create all active subscriptions via `batch_import_subscriptions` (if
   available) or individual `subscribe` calls funded from the migration wallet.
3. Verify each subscriber's balance matches the V1 export.

```bash
# Verify solvency: total token balances must match
python3 scripts/verify_migration_solvency.py \
  --v1-export v1_subscriptions.json \
  --v2-contract <V2_CONTRACT_ID> \
  --network testnet
```

### Phase 5 — Announce Migration

Post the V2 contract address to all official channels. Update the README and
deployed contract documentation. Archive V1 contract address as deprecated.

### Solvency Invariant

At the end of migration, the following must hold for every subscriber:

```
v2_subscriber_balance >= v1_subscriber_balance_at_freeze_time
```

Any shortfall must be covered by the DAO treasury before the migration is
declared complete.

---

## 10. Post-Incident Checklist

Complete this checklist within 48 hours of resolving any P0 or P1 incident.

- [ ] Root cause identified and documented
- [ ] All emergency bypass actions justified in writing
- [ ] Affected users notified with accurate information
- [ ] Financial impact quantified (tokens at risk, tokens lost if any)
- [ ] Patch developed, reviewed, and tested
- [ ] Patch deployed to Testnet and verified for ≥ 48 hours
- [ ] Patch deployed to Mainnet (if applicable)
- [ ] Post-mortem published to DAO forum within 7 days
- [ ] Security Council debrief completed
- [ ] This runbook updated if any procedure was found to be incorrect
- [ ] Audit firm notified if the incident reveals a previously unknown vulnerability

---

*Last reviewed: 2026-04-27 | Maintained by the SubStream Security Council*
*All key-holders must acknowledge receipt of this document annually.*
