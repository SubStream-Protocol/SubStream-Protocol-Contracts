# SubStream Protocol — Webhook Integration Guide

This guide explains how to listen to SubStream on-chain events and forward them to your Web2 backend as webhooks, enabling real-time provisioning and revocation of user access without polling the blockchain.

---

## Overview

SubStream emits structured Soroban contract events on every billing action. Each event includes a `merchant_reference_id` — a string you set on-chain that maps a subscriber's Stellar pubkey to your internal user ID (e.g. a Stripe customer ID, UUID, or email hash).

```
Subscriber pubkey  ──►  merchant_reference_id  ──►  Your DB user record
```

---

## Setting a Reference ID

Before a subscriber's first billing cycle, store their Web2 reference ID on-chain:

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --source <MERCHANT_SECRET_KEY> \
  --network testnet \
  -- set_merchant_reference_id \
  --merchant <MERCHANT_ADDRESS> \
  --subscriber <SUBSCRIBER_ADDRESS> \
  --reference_id "usr_1234abcd"
```

This is idempotent — call it again to update the reference ID.

---

## Key Events

### `SubscriptionBilled`

Emitted on every successful payment pull.

| Field | Type | Description |
|---|---|---|
| `subscriber` | Address | Subscriber's Stellar pubkey |
| `merchant` | Address | Merchant's Stellar pubkey |
| `amount` | i128 | Amount charged (in token stroops) |
| `billed_at` | u64 | Unix timestamp of the charge |
| `merchant_reference_id` | String | Your Web2 user identifier |
| `receipt_hash` | BytesN<32> | Cryptographic receipt (see [Receipt Verification](#receipt-verification)) |

### `PaymentFailedGracePeriodStarted`

Emitted when a payment attempt fails due to insufficient allowance.

| Field | Type | Description |
|---|---|---|
| `subscriber` | Address | Subscriber's Stellar pubkey |
| `merchant` | Address | Merchant's Stellar pubkey |
| `dunning_start_timestamp` | u64 | When the grace period started |
| `grace_period_end` | u64 | Deadline before subscription expires |
| `merchant_reference_id` | String | Your Web2 user identifier |

### `TrialStarted`

Emitted when a subscriber begins a free trial.

| Field | Type | Description |
|---|---|---|
| `subscriber` | Address | Subscriber's Stellar pubkey |
| `merchant` | Address | Merchant's Stellar pubkey |
| `trial_duration` | u64 | Trial length in seconds |
| `started_at` | u64 | Unix timestamp |
| `merchant_reference_id` | String | Your Web2 user identifier |

---

## Listening to Events (Node.js)

```typescript
import { SorobanRpc, xdr } from "@stellar/stellar-sdk";

const server = new SorobanRpc.Server("https://soroban-testnet.stellar.org");
const CONTRACT_ID = "CAOUX2FZ65IDC4F2X7LJJ2SVF23A35CCTZB7KVVN475JCLKTTU4CEY6L";

async function pollEvents(startLedger: number) {
  const events = await server.getEvents({
    startLedger,
    filters: [
      {
        type: "contract",
        contractIds: [CONTRACT_ID],
        topics: [
          // SubscriptionBilled: topic[0] = subscriber, topic[1] = merchant, topic[2] = amount
          ["*", "*", "*"],
        ],
      },
    ],
  });

  for (const event of events.events) {
    const eventName = event.topic[0]?.value(); // first topic is the event name in some SDKs
    await dispatchWebhook(event);
  }
}

async function dispatchWebhook(event: any) {
  const payload = {
    event_type: "subscription.billed",
    subscriber: event.value.subscriber,
    merchant: event.value.merchant,
    amount: event.value.amount,
    billed_at: event.value.billed_at,
    merchant_reference_id: event.value.merchant_reference_id,
    receipt_hash: event.value.receipt_hash,
  };

  // Forward to your backend — use HTTPS and verify with a shared secret
  await fetch("https://your-backend.example.com/webhooks/substream", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "X-SubStream-Signature": computeHmac(payload, process.env.WEBHOOK_SECRET!),
    },
    body: JSON.stringify(payload),
  });
}
```

---

## Zapier / No-Code Integration

1. Use the **Stellar Horizon** or **Soroban RPC** Zapier app to trigger on new contract events.
2. Filter by `contractId = <CONTRACT_ID>`.
3. Map `merchant_reference_id` to your CRM's user ID field.
4. Add a Zapier action to provision/revoke access in your app (e.g. update a Stripe subscription, toggle a feature flag).

---

## Receipt Verification

Every `SubscriptionBilled` event includes a `receipt_hash`:

```
receipt_hash = sha256(merchant_bytes || subscriber_bytes || amount_be16 || billed_at_be8 || cycle_number_be8)
```

To verify a PDF invoice matches the on-chain record:

```typescript
import { createHash } from "crypto";
import { StrKey } from "@stellar/stellar-sdk";

function verifyReceipt(
  merchantAddress: string,
  subscriberAddress: string,
  amount: bigint,
  billedAt: bigint,
  cycleNumber: bigint,
  expectedHash: string
): boolean {
  const merchantBytes = StrKey.decodeEd25519PublicKey(merchantAddress);
  const subscriberBytes = StrKey.decodeEd25519PublicKey(subscriberAddress);

  const buf = Buffer.alloc(32 + 32 + 16 + 8 + 8);
  merchantBytes.copy(buf, 0);
  subscriberBytes.copy(buf, 32);
  buf.writeBigInt64BE(amount, 64);
  buf.writeBigUInt64BE(billedAt, 80);
  buf.writeBigUInt64BE(cycleNumber, 88);

  const hash = createHash("sha256").update(buf).digest("hex");
  return hash === expectedHash;
}
```

---

## Security Considerations

- **Reference ID injection**: The `merchant_reference_id` is a plain string stored on-chain. Never use it directly in SQL queries — always use parameterized queries in your backend.
- **Webhook authenticity**: Always verify the HMAC signature on incoming webhook payloads before processing them.
- **Replay attacks**: Include the `billed_at` timestamp in your HMAC and reject payloads older than 5 minutes.
- **Idempotency**: Use `receipt_hash` as an idempotency key to prevent double-provisioning if your webhook endpoint receives duplicate deliveries.
