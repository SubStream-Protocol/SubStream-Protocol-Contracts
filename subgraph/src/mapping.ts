import { BigInt, Bytes } from "@graphprotocol/graph-ts";
import {
  SubscriptionBilled,
  TrialStarted,
  PaymentFailedGracePeriodStarted,
  B2BReceiptIssued,
  SubscriptionCreated,
  Unsubscribed,
} from "../generated/SubStreamProtocol/SubStreamProtocol";
import {
  Subscription,
  Merchant,
  BillingEvent,
  Receipt,
  PaymentFailure,
  ProtocolStats,
} from "../generated/schema";

function subscriptionId(subscriber: Bytes, merchant: Bytes): string {
  return subscriber.toHex() + "-" + merchant.toHex();
}

function loadOrCreateMerchant(address: Bytes): Merchant {
  let id = address.toHex();
  let m = Merchant.load(id);
  if (!m) {
    m = new Merchant(id);
    m.address = address;
    m.totalRevenue = BigInt.zero();
    m.activeSubscribers = BigInt.zero();
    m.totalSubscribers = BigInt.zero();
    m.save();
  }
  return m as Merchant;
}

function loadOrCreateStats(): ProtocolStats {
  let stats = ProtocolStats.load("global");
  if (!stats) {
    stats = new ProtocolStats("global");
    stats.totalVolume = BigInt.zero();
    stats.totalSubscriptions = BigInt.zero();
    stats.activeSubscriptions = BigInt.zero();
    stats.save();
  }
  return stats as ProtocolStats;
}

export function handleSubscriptionCreated(event: SubscriptionCreated): void {
  let merchant = loadOrCreateMerchant(event.params.merchant);
  merchant.totalSubscribers = merchant.totalSubscribers.plus(BigInt.fromI32(1));
  merchant.activeSubscribers = merchant.activeSubscribers.plus(BigInt.fromI32(1));
  merchant.save();

  let id = subscriptionId(event.params.subscriber, event.params.merchant);
  let sub = new Subscription(id);
  sub.subscriber = event.params.subscriber;
  sub.merchant = merchant.id;
  sub.status = "Active";
  sub.merchantReferenceId = "";
  sub.createdAt = event.params.created_at;
  sub.updatedAt = event.params.created_at;
  sub.save();

  let stats = loadOrCreateStats();
  stats.totalSubscriptions = stats.totalSubscriptions.plus(BigInt.fromI32(1));
  stats.activeSubscriptions = stats.activeSubscriptions.plus(BigInt.fromI32(1));
  stats.save();
}

export function handleTrialStarted(event: TrialStarted): void {
  let id = subscriptionId(event.params.subscriber, event.params.merchant);
  let sub = Subscription.load(id);
  if (!sub) return;
  sub.status = "Trial";
  sub.merchantReferenceId = event.params.merchant_reference_id;
  sub.updatedAt = event.params.started_at;
  sub.save();
}

export function handleSubscriptionBilled(event: SubscriptionBilled): void {
  let merchant = loadOrCreateMerchant(event.params.merchant);
  merchant.totalRevenue = merchant.totalRevenue.plus(event.params.amount);
  merchant.save();

  let subId = subscriptionId(event.params.subscriber, event.params.merchant);
  let sub = Subscription.load(subId);
  if (!sub) return;
  sub.status = "Active";
  sub.merchantReferenceId = event.params.merchant_reference_id;
  sub.updatedAt = event.params.billed_at;
  sub.save();

  let billingId = event.transaction.hash.toHex() + "-" + event.logIndex.toString();
  let billing = new BillingEvent(billingId);
  billing.subscription = subId;
  billing.subscriber = event.params.subscriber;
  billing.merchant = merchant.id;
  billing.amount = event.params.amount;
  billing.billedAt = event.params.billed_at;
  billing.merchantReferenceId = event.params.merchant_reference_id;
  billing.receiptHash = event.params.receipt_hash;
  billing.save();

  let stats = loadOrCreateStats();
  stats.totalVolume = stats.totalVolume.plus(event.params.amount);
  stats.save();
}

export function handleB2BReceiptIssued(event: B2BReceiptIssued): void {
  let subId = subscriptionId(event.params.subscriber, event.params.merchant);
  let receiptId = event.params.receipt_hash.toHex();
  let receipt = new Receipt(receiptId);
  receipt.subscription = subId;
  receipt.subscriber = event.params.subscriber;
  receipt.merchant = event.params.merchant.toHex();
  receipt.receiptHash = event.params.receipt_hash;
  receipt.amount = event.params.amount;
  receipt.cycleNumber = event.params.cycle_number;
  receipt.issuedAt = event.params.issued_at;
  receipt.save();
}

export function handlePaymentFailed(event: PaymentFailedGracePeriodStarted): void {
  let subId = subscriptionId(event.params.subscriber, event.params.merchant);
  let sub = Subscription.load(subId);
  if (sub) {
    sub.status = "PastDue";
    sub.merchantReferenceId = event.params.merchant_reference_id;
    sub.updatedAt = event.params.dunning_start_timestamp;
    sub.save();
  }

  let failureId = event.transaction.hash.toHex() + "-" + event.logIndex.toString();
  let failure = new PaymentFailure(failureId);
  failure.subscriber = event.params.subscriber;
  failure.merchant = event.params.merchant.toHex();
  failure.merchantReferenceId = event.params.merchant_reference_id;
  failure.dunningStartTimestamp = event.params.dunning_start_timestamp;
  failure.gracePeriodEnd = event.params.grace_period_end;
  failure.save();
}

export function handleUnsubscribed(event: Unsubscribed): void {
  let id = subscriptionId(event.params.subscriber, event.params.creator);
  let sub = Subscription.load(id);
  if (!sub) return;
  sub.status = "Canceled";
  sub.updatedAt = BigInt.zero(); // timestamp not in event; set to 0
  sub.save();

  let merchant = Merchant.load(event.params.creator.toHex());
  if (merchant && merchant.activeSubscribers.gt(BigInt.zero())) {
    merchant.activeSubscribers = merchant.activeSubscribers.minus(BigInt.fromI32(1));
    merchant.save();
  }

  let stats = loadOrCreateStats();
  if (stats.activeSubscriptions.gt(BigInt.zero())) {
    stats.activeSubscriptions = stats.activeSubscriptions.minus(BigInt.fromI32(1));
  }
  stats.save();
}
