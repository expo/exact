import { expect, test } from 'bun:test';
import { implementation } from '../web/index.js';

function withPaymentRequest(createStub) {
  const saved = globalThis.PaymentRequest;
  globalThis.PaymentRequest = createStub;
  return () => {
    if (saved === undefined) delete globalThis.PaymentRequest;
    else globalThis.PaymentRequest = saved;
  };
}

const baseArgs = {
  methodData: [{ supportedMethods: 'https://google.com/pay', data: '{}' }],
  details: { id: 'ord-1', total: { label: 'Total', amount: { currency: 'USD', value: '10.00' } } },
  onpaymentauthorized: 'authorize',
  onshippingaddresschange: 'ship',
  onpaymentmethodchange: 'method',
};

test('Path A: cancel during onpaymentauthorized prevents retry() and completes as fail', async () => {
  let abortCalls = 0, retryCalls = 0, authCalls = 0;
  const completeCalls = [], logs = [];
  let resolveAuth;
  const restore = withPaymentRequest(class {
    constructor() { this.id = 'ord-1'; this.state = 'created'; }
    async show() {
      this.state = 'closed';
      return {
        methodName: 'https://google.com/pay', details: {}, shippingAddress: null, shippingOption: null,
        payerName: null, payerEmail: null, payerPhone: null,
        async retry() { retryCalls++; },
        async complete(r) { completeCalls.push(r); },
      };
    }
    async abort() {
      abortCalls++;
      if (this.state === 'closed') throw new DOMException('closed', 'InvalidStateError');
    }
  });
  try {
    const impl = implementation({
      log: line => logs.push(line),
      invoke: () => {
        authCalls++;
        return new Promise(r => { resolveAuth = r; });
      },
    });
    const pending = impl.showPaymentRequest(baseArgs, { request: 1 });
    await Bun.sleep(0);
    expect(authCalls).toBe(1);
    impl.cancel(1);
    resolveAuth({ text: JSON.stringify({ retry: { error: 'Try another card' } }) });
    const out = await pending;
    expect(out).toEqual({ error: 'AbortError' });
    expect(authCalls).toBe(1);
    expect(retryCalls).toBe(0);
    expect(abortCalls).toBe(0);
    expect(completeCalls).toEqual(['fail']);
    expect(logs.some(l => l.includes('closed after its call ended'))).toBe(true);
  } finally {
    restore();
  }
});

test('Path B: cancel while awaiting PaymentResponse.retry() completes as fail without re-invoking onpaymentauthorized', async () => {
  let abortCalls = 0, retryCalls = 0, authCalls = 0, updateInvokes = 0;
  const completeCalls = [], logs = [];
  let resolveRetry, prInstance;
  const restore = withPaymentRequest(class {
    constructor() { this.id = 'ord-2'; this.state = 'created'; prInstance = this; }
    async show() {
      this.state = 'closed';
      return {
        methodName: 'https://google.com/pay', details: {}, shippingAddress: null, shippingOption: null,
        payerName: null, payerEmail: null, payerPhone: null,
        retry() { retryCalls++; return new Promise(r => { resolveRetry = r; }); },
        async complete(r) { completeCalls.push(r); },
      };
    }
    async abort() {
      abortCalls++;
      if (this.state === 'closed') throw new DOMException('closed', 'InvalidStateError');
    }
  });
  try {
    const impl = implementation({
      log: line => logs.push(line),
      invoke: (_handle, handler) => {
        if (handler === 'authorize') {
          authCalls++;
          return Promise.resolve({ text: JSON.stringify(authCalls === 1 ? { retry: { error: 'Bad ZIP' } } : { result: 'success' }) });
        }
        updateInvokes++;
        return Promise.resolve({ text: '{}' });
      },
    });
    const pending = impl.showPaymentRequest(baseArgs, { request: 2 });
    await Bun.sleep(0);
    expect(authCalls).toBe(1);
    expect(retryCalls).toBe(1);

    // Cancelled while retry sheet is open.
    impl.cancel(2);

    // Shipping / payment-method events arriving after cancel must not call app handlers.
    let updatedWith;
    prInstance.onshippingaddresschange({ updateWith: p => { updatedWith = p; } });
    await updatedWith;
    prInstance.onpaymentmethodchange({ updateWith: p => { updatedWith = p; } });
    await updatedWith;
    expect(updateInvokes).toBe(0);

    // User resubmits on the retry sheet -> must complete('fail') without calling onpaymentauthorized again.
    resolveRetry();
    const out = await pending;
    expect(out).toEqual({ error: 'AbortError' });
    expect(authCalls).toBe(1);
    expect(retryCalls).toBe(1);
    expect(abortCalls).toBe(0);
    expect(completeCalls).toEqual(['fail']);
    expect(logs.some(l => l.includes('closed after its call ended'))).toBe(true);
  } finally {
    restore();
  }
});

test('cancel during onpaymentauthorized lets a final success answer complete the sheet', async () => {
  const completeCalls = [], logs = [];
  let resolveAuth;
  const restore = withPaymentRequest(class {
    constructor() { this.id = 'ord-3'; }
    async show() {
      return {
        methodName: 'https://google.com/pay', details: {}, shippingAddress: null, shippingOption: null,
        payerName: null, payerEmail: null, payerPhone: null,
        async retry() { throw new Error('unexpected retry'); },
        async complete(r) { completeCalls.push(r); },
      };
    }
    async abort() {}
  });
  try {
    const impl = implementation({
      log: line => logs.push(line),
      invoke: () => new Promise(r => { resolveAuth = r; }),
    });
    const pending = impl.showPaymentRequest(baseArgs, { request: 3 });
    await Bun.sleep(0);
    impl.cancel(3);
    resolveAuth({ text: JSON.stringify({ result: 'success', data: 'ok' }) });
    const out = await pending;
    expect(out.result).toBe('success');
    expect(out.data).toBe('"ok"');
    expect(completeCalls).toEqual(['success']);
    expect(logs.some(l => l.includes('last result success'))).toBe(true);
  } finally {
    restore();
  }
});
