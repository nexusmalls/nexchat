import { describeValue } from './codec.js';
import { TxEvent, TxReceipt } from './api.js';

export class TestAssertionError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'TestAssertionError';
  }
}

export function assert(condition: unknown, message: string): asserts condition {
  if (!condition) {
    throw new TestAssertionError(message);
  }
}

export function assertEqual<T>(actual: T, expected: T, message: string): void {
  if (actual !== expected) {
    throw new TestAssertionError(`${message}: expected=${describeValue(expected)} actual=${describeValue(actual)}`);
  }
}

export function assertTxSuccess(receipt: TxReceipt, message: string): void {
  if (!receipt.success) {
    throw new TestAssertionError(`${message}: ${receipt.error ?? 'unknown tx failure'}`);
  }
}

export function findEvent(receipt: TxReceipt, section: string, method: string): TxEvent | undefined {
  return receipt.events.find((event) => event.section === section && event.method === method);
}

export function assertEvent(receipt: TxReceipt, section: string, method: string, message: string): TxEvent {
  const event = findEvent(receipt, section, method);
  if (!event) {
    const actual = receipt.events.map((item) => `${item.section}.${item.method}`).join(', ');
    throw new TestAssertionError(`${message}: missing ${section}.${method}, actual=[${actual}]`);
  }
  return event;
}
