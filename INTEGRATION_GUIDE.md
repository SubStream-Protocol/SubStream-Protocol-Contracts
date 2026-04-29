# SubStream Protocol: Merchant Integration Guide

This guide walks Web2 businesses through integrating SubStream's decentralized, pay-as-you-go billing into their existing product. No prior blockchain experience is required.

---

## Table of Contents

1. [How SubStream Billing Works](#1-how-substream-billing-works)
2. [Architecture Overview](#2-architecture-overview)
3. [Prerequisites](#3-prerequisites)
4. [Setting Up the TypeScript SDK](#4-setting-up-the-typescript-sdk)
5. [Triggering a Subscription (Frontend)](#5-triggering-a-subscription-frontend)
6. [Collecting Revenue (Backend)](#6-collecting-revenue-backend)
7. [Handling Webhooks & Dunning](#7-handling-webhooks--dunning)
8. [Grace Periods & Cancellations](#8-grace-periods--cancellations)
9. [Security: Verifying Slippage Caps](#9-security-verifying-slippage-caps)
10. [Glossary](#10-glossary)

---

## 1. How SubStream Billing Works

| Traditional SaaS | SubStream |
|---|---|
| Monthly credit card charge | Continuous token stream (per-second) |
| Charge fails → dunning emails | Balance runs low → grace period |
| Refund requires support ticket | Cancel instantly, unspent balance auto-refunded |
| Vendor holds funds | Funds held in smart contract, not by you |

**Key concept:** Your customer deposits a token buffer (e.g., 50 XLM) into the smart contract and sets a rate (e.g., 0.001 XLM/second). The contract streams tokens to your vault second-by-second. You call `collect` to withdraw accumulated earnings at any time.

**7-day free trial:** For the first 7 days after a user subscribes, the stream accrues zero charges. After the trial ends, billing begins automatically at the configured paid rate — no action required from you or the user.

---

## 2. Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        USER BROWSER                             │
│                                                                 │
│  ┌──────────────┐   signs tx   ┌──────────────────────────────┐ │
│  │  Your React  │ ──────────►  │  Freighter / Lobstr Wallet   │ │
│  │  Frontend    │              └──────────────────────────────┘ │
└──────────┬──────────────────────────────────────────────────────┘
           │ submit signed tx
           ▼
┌─────────────────────────────────────────────────────────────────┐
│                    STELLAR NETWORK (Testnet)                     │
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │          SubStream Smart Contract                        │   │
│  │  CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L│  │
│  │                                                          │   │
│  │  subscribe() ──► streams tokens per second              │   │
│  │  collect()   ──► sends accrued XLM to merchant vault    │   │
│  │  cancel()    ──► refunds unspent balance to user        │   │
│  └──────────────────────────────────────────────────────────┘   │
           │ events emitted
           ▼
┌─────────────────────────────────────────────────────────────────┐
│                    YOUR BACKEND SERVER                          │
│                                                                 │
│  Horizon event listener → webhook handler → grant/revoke access │
└─────────────────────────────────────────────────────────────────┘
```

**Flow of funds:**

```
User Wallet
    │
    │  deposit buffer (e.g. 50 XLM)
    ▼
Smart Contract Escrow
    │
    │  streams per-second to merchant vault
    ▼
Merchant Vault Address
    │
    │  collect() call from your backend
    ▼
Your Treasury Wallet
```

---

## 3. Prerequisites

- Node.js 18+ and npm/yarn
- A Stellar account (your **merchant vault address**) funded on Testnet
  - Get free Testnet XLM: `stellar friendbot --network testnet <YOUR_ADDRESS>`
- The SubStream contract ID: `CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L`

---

## 4. Setting Up the TypeScript SDK

```bash
npm install @stellar/stellar-sdk
```

Create a shared config file:

```typescript
// src/substream/config.ts
import { Contract, Networks, rpc } from "@stellar/stellar-sdk";

export const CONTRACT_ID =
  "CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L";

export const NETWORK_PASSPHRASE = Networks.TESTNET;

export const RPC_URL = "https://soroban-testnet.stellar.org";

export const rpcServer = new rpc.Server(RPC_URL, { allowHttp: false });

export const contract = new Contract(CONTRACT_ID);
```

---

## 5. Triggering a Subscription (Frontend)

This is the code your **Subscribe** button calls. It builds the Soroban authorization payload and asks the user's wallet to sign it.

```typescript
// src/substream/subscribe.ts
import {
  TransactionBuilder,
  BASE_FEE,
  Keypair,
  nativeToScVal,
  Address,
  xdr,
} from "@stellar/stellar-sdk";
import { CONTRACT_ID, NETWORK_PASSPHRASE, RPC_URL, rpcServer, contract } from "./config";

/**
 * Subscribes a user to a creator's channel.
 *
 * @param userPublicKey  - The subscriber's Stellar public key (G...)
 * @param creatorAddress - The creator/merchant vault address (G...)
 * @param bufferAmount   - Token buffer in stroops (1 XLM = 10_000_000 stroops)
 * @param ratePerSecond  - Streaming rate in stroops per second
 * @param signTransaction - Wallet adapter function (e.g. from Freighter)
 */
export async function executeSubscriptionPull(
  userPublicKey: string,
  creatorAddress: string,
  bufferAmount: bigint,
  ratePerSecond: bigint,
  signTransaction: (xdr: string) => Promise<string>
): Promise<string> {
  // 1. Fetch the user's current account state from the network
  const account = await rpcServer.getAccount(userPublicKey);

  // 2. Build the transaction invoking the subscribe function
  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK_PASSPHRASE,
  })
    .addOperation(
      contract.call(
        "subscribe",
        // subscriber address
        new Address(userPublicKey).toScVal(),
        // creator/merchant address
        new Address(creatorAddress).toScVal(),
        // buffer deposit amount
        nativeToScVal(bufferAmount, { type: "i128" }),
        // per-second streaming rate
        nativeToScVal(ratePerSecond, { type: "i128" })
      )
    )
    .setTimeout(30)
    .build();

  // 3. Simulate to get the Soroban authorization entries the wallet must sign
  const simResult = await rpcServer.simulateTransaction(tx);
  if (!("result" in simResult)) {
    throw new Error(`Simulation failed: ${JSON.stringify(simResult)}`);
  }

  // 4. Assemble the transaction with the auth entries from simulation
  const { assembleTransaction } = await import("@stellar/stellar-sdk/rpc");
  const assembledTx = assembleTransaction(tx, simResult);

  // 5. Ask the user's wallet to sign (Freighter, Lobstr, etc.)
  const signedXdr = await signTransaction(assembledTx.toXDR());

  // 6. Submit to the network
  const submitResult = await rpcServer.sendTransaction(
    TransactionBuilder.fromXDR(signedXdr, NETWORK_PASSPHRASE)
  );

  if (submitResult.status === "ERROR") {
    throw new Error(`Submission failed: ${submitResult.errorResult}`);
  }

  return submitResult.hash;
}
```

### Slippage cap — client-side validation

Before calling `executeSubscriptionPull`, validate that the rate the user is about to sign matches what your UI displayed. This prevents a compromised RPC node from substituting a higher rate:

```typescript
// SECURITY: Always validate rate before signing — see Section 9
const MAX_RATE_STROOPS = BigInt(10_000); // your plan's maximum rate

if (ratePerSecond > MAX_RATE_STROOPS) {
  throw new Error("Rate exceeds plan maximum. Aborting to protect user.");
}
```

---

## 6. Collecting Revenue (Backend)

Call `collect` from your backend on a schedule (e.g., daily cron) to pull accrued tokens into your vault.

```typescript
// src/substream/collect.ts
import {
  TransactionBuilder,
  BASE_FEE,
  Keypair,
  Address,
} from "@stellar/stellar-sdk";
import { NETWORK_PASSPHRASE, rpcServer, contract } from "./config";

/**
 * Collects all accrued streaming revenue for a creator.
 * Call this from a trusted backend process using the merchant's secret key.
 *
 * @param merchantSecretKey - The merchant vault's Stellar secret key (S...)
 * @param subscriberAddress - The subscriber whose stream to collect from
 */
export async function collectRevenue(
  merchantSecretKey: string,
  subscriberAddress: string
): Promise<string> {
  const merchantKeypair = Keypair.fromSecret(merchantSecretKey);
  const account = await rpcServer.getAccount(merchantKeypair.publicKey());

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK_PASSPHRASE,
  })
    .addOperation(
      contract.call(
        "collect",
        new Address(merchantKeypair.publicKey()).toScVal(),
        new Address(subscriberAddress).toScVal()
      )
    )
    .setTimeout(30)
    .build();

  const simResult = await rpcServer.simulateTransaction(tx);
  const { assembleTransaction } = await import("@stellar/stellar-sdk/rpc");
  const assembledTx = assembleTransaction(tx, simResult);

  assembledTx.sign(merchantKeypair);

  const result = await rpcServer.sendTransaction(assembledTx);
  return result.hash;
}
```

> **Tip:** Store `merchantSecretKey` in an environment variable or secrets manager — never hardcode it.

---

## 7. Handling Webhooks & Dunning

SubStream emits on-chain events when a subscription is created, collected, or cancelled. Poll the Horizon API to react to these events in your backend.

### Listening for events

```typescript
// src/substream/eventListener.ts
import Horizon from "@stellar/stellar-sdk/horizon";

const horizonServer = new Horizon.Server("https://horizon-testnet.stellar.org");
const CONTRACT_ID = "CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L";

/**
 * Streams contract events and calls your handler for each one.
 * Run this as a long-lived background process.
 */
export function startEventListener(
  onEvent: (event: Horizon.ServerApi.EffectRecord) => void
) {
  horizonServer
    .effects()
    .forAccount(CONTRACT_ID)
    .cursor("now")
    .stream({
      onmessage: onEvent,
      onerror: (err) => console.error("Stream error:", err),
    });
}
```

### Dunning: handling low-balance subscribers

In traditional SaaS, "dunning" means retrying failed payments. In SubStream, the equivalent is the **grace period** — when a subscriber's buffer runs low, the contract enters a grace period before the stream expires.

Your dunning flow:

```
Contract event: balance_low
        │
        ▼
Your backend receives event
        │
        ├─► Send email: "Your SubStream balance is running low. Top up to keep access."
        │
        └─► Start a 24-hour countdown timer
                │
                ├─► If top_up event received → cancel timer, restore access
                │
                └─► If timer expires → revoke user's access in your database
```

```typescript
// src/substream/dunning.ts

interface SubscriberRecord {
  userId: string;
  email: string;
  stellarAddress: string;
}

export async function handleLowBalanceEvent(subscriber: SubscriberRecord) {
  // 1. Notify the user
  await sendEmail(subscriber.email, {
    subject: "Your SubStream balance is running low",
    body: `Top up your balance to keep your subscription active. 
           Visit your account page to add more XLM.`,
  });

  // 2. Schedule access revocation after grace period
  await scheduleJob({
    runAt: Date.now() + 24 * 60 * 60 * 1000, // 24 hours
    jobType: "revoke_access",
    payload: { userId: subscriber.userId },
  });
}

export async function handleTopUpEvent(subscriber: SubscriberRecord) {
  // Cancel any pending revocation job
  await cancelJob({ jobType: "revoke_access", userId: subscriber.userId });
  // Ensure access is active
  await grantAccess(subscriber.userId);
}

// Stubs — replace with your email/job/access implementations
async function sendEmail(_to: string, _msg: object) {}
async function scheduleJob(_job: object) {}
async function cancelJob(_filter: object) {}
async function grantAccess(_userId: string) {}
```

---

## 8. Grace Periods & Cancellations

| SaaS Term | SubStream Equivalent |
|---|---|
| Payment failed | Balance depleted / grace period entered |
| Subscription paused | `pause_channel` called by creator |
| Subscription cancelled | `cancel` called by subscriber |
| Refund issued | Unspent buffer auto-returned on `cancel` |
| Minimum commitment | 24-hour minimum stream duration (sybil protection) |

**Important:** A subscriber cannot cancel within the first 24 hours of a stream. This is enforced by the contract to prevent content scraping. Do not promise instant cancellation in your UI — instead, communicate the 24-hour minimum commitment clearly.

```typescript
// Show this in your cancellation UI
const MIN_DURATION_HOURS = 24;

export function canCancelNow(subscriptionStartTimestamp: number): boolean {
  const elapsed = Date.now() - subscriptionStartTimestamp;
  return elapsed >= MIN_DURATION_HOURS * 60 * 60 * 1000;
}
```

---

## 9. Security: Verifying Slippage Caps

A slippage cap ensures the user never signs a transaction with a higher rate than what your UI displayed. This is critical because:

- A compromised or malicious RPC node could return manipulated simulation results.
- A UI bug could pass the wrong rate to the transaction builder.

**Always validate on the client before presenting the transaction to the wallet:**

```typescript
// src/substream/validation.ts

export interface SubscriptionParams {
  bufferAmount: bigint;   // in stroops
  ratePerSecond: bigint;  // in stroops
}

export interface PlanLimits {
  maxBufferAmount: bigint;
  maxRatePerSecond: bigint;
}

/**
 * Throws if the subscription params exceed the plan's allowed limits.
 * Call this before building the transaction.
 */
export function validateSlippageCap(
  params: SubscriptionParams,
  limits: PlanLimits
): void {
  if (params.ratePerSecond > limits.maxRatePerSecond) {
    throw new Error(
      `Rate ${params.ratePerSecond} exceeds plan maximum ${limits.maxRatePerSecond}. ` +
      `Transaction aborted to protect user.`
    );
  }
  if (params.bufferAmount > limits.maxBufferAmount) {
    throw new Error(
      `Buffer ${params.bufferAmount} exceeds plan maximum ${limits.maxBufferAmount}. ` +
      `Transaction aborted to protect user.`
    );
  }
}
```

---

## 10. Glossary

| Term | Meaning |
|---|---|
| **Buffer** | The upfront token deposit a subscriber makes (like a prepaid balance) |
| **Rate** | Tokens streamed per second to the merchant |
| **Stroops** | The smallest XLM unit: 1 XLM = 10,000,000 stroops |
| **Collect** | Merchant action to withdraw accrued streaming revenue |
| **Grace Period** | Window after balance runs low before the stream expires |
| **Dunning** | The process of notifying and recovering lapsing subscribers |
| **Soroban** | Stellar's smart contract platform |
| **Vault Address** | Your merchant Stellar account that receives streaming payments |
| **Trial Period** | First 7 days of a subscription — zero charges accrue |
| **Minimum Duration** | 24-hour lock-in after subscribing (sybil protection) |
