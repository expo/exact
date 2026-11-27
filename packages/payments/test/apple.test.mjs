import { expect, test } from 'bun:test';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const root = resolve(import.meta.dir, '../../..');

test.skipIf(process.platform !== 'darwin')('Apple PaymentsModule handles explicit JSON nulls and primitive data without crashing', () => {
  mkdirSync(join(root, 'target'), { recursive: true });
  const out = mkdtempSync(join(root, 'target/payments-apple-nulls-'));
  const run = (cmd, args) => {
    const r = spawnSync(cmd, args, { cwd: root, encoding: 'utf8', timeout: 120_000 });
    expect(r.status, `${cmd}: ${r.stdout}\n${r.stderr}`).toBe(0);
    return r.stdout;
  };
  try {
    const source = join(out, 'Check.swift');
    writeFileSync(source, String.raw`import ExactPackage
import Foundation
import PassKit

private func wait(_ ready: () -> Bool) {
    let limit = Date().addingTimeInterval(5)
    while !ready() && Date() < limit { _ = RunLoop.main.run(mode: .default, before: limit) }
    precondition(ready(), "timed out waiting for callback")
}

private final class FakePaymentToken: PKPaymentToken {
    override var paymentData: Data { Data("{}".utf8) }
    override var transactionIdentifier: String { "tx-1" }
    override var paymentMethod: PKPaymentMethod { PKPaymentMethod() }
}

private final class FakePayment: PKPayment {
    private let tok = FakePaymentToken()
    override var token: PKPaymentToken { tok }
}

@main struct Check {
    static func main() {
        // 1. paymentDataJSON handles NSNull, primitives, containers, and invalid numbers without throwing NSInvalidArgumentException.
        precondition(paymentDataJSON(nil) == nil)
        precondition(paymentDataJSON(NSNull()) == nil)
        precondition(paymentDataJSON("order-123") == "\"order-123\"")
        precondition(paymentDataJSON(42) == "42")
        precondition(paymentDataJSON(true) == "true")
        precondition(paymentDataJSON(["id": "o1"]) == "{\"id\":\"o1\"}")
        precondition(paymentDataJSON([1, 2]) == "[1,2]")
        precondition(paymentDataJSON(Double.nan) == nil)

        // 2. Shipping update with explicit NSNull preserves existing total/displayItems and produces no errors or dismissal.
        var handlerReply = ""
        let ctx = ExactPackageContext(agent: false, session: "pay", changed: { _ in }, log: { _ in }, now: { 0 },
                                      invoke: { _, _, _, reply in reply(["text": handlerReply]) })
        let mod = PaymentsModule(context: ctx)
        let details: [String: Any] = [
            "id": "ord-1",
            "total": ["label": "Shop", "amount": ["currency": "USD", "value": "25.00"]],
            "displayItems": [["label": "Book", "amount": ["currency": "USD", "value": "25.00"]]],
        ]
        let args: [String: Any] = [
            "details": details,
            "onshippingaddresschange": "onShip",
            "onpaymentauthorized": "onAuth",
        ]
        let req = PKPaymentRequest()
        let controller = PKPaymentAuthorizationController(paymentRequest: req)
        var dismissedReplies: [[String: Any]] = []
        let initialItems = [
            PKPaymentSummaryItem(label: "Book", amount: NSDecimalNumber(string: "25.00")),
            PKPaymentSummaryItem(label: "Shop", amount: NSDecimalNumber(string: "25.00")),
        ]
        let sheet = PaymentsModule.Sheet(request: 1, id: "ord-1", args: args, controller: controller, items: initialItems, methods: []) {
            dismissedReplies.append($0)
        }
        mod.sheet = sheet

        handlerReply = #"{"total":null,"displayItems":null,"shippingOptions":[{"id":"std","label":"Standard","amount":{"currency":"USD","value":"5.00"}}],"error":null,"shippingAddressErrors":{"postalCode":null}}"#
        var shipUpdate: PKPaymentRequestShippingContactUpdate?
        mod.paymentAuthorizationController(controller, didSelectShippingContact: PKContact()) { shipUpdate = $0 }
        wait { shipUpdate != nil }
        precondition(shipUpdate!.errors.isEmpty, "explicit null errors must produce no PassKit errors")
        precondition(shipUpdate!.paymentSummaryItems.count == 2, "explicit null total/displayItems must keep original summary items")
        precondition(shipUpdate!.paymentSummaryItems.last?.amount == NSDecimalNumber(string: "25.00"), "original total must be preserved")
        precondition(shipUpdate!.shippingMethods.count == 1, "new shipping options must be applied")
        var drained = false
        DispatchQueue.main.async { drained = true }
        wait { drained }
        precondition(dismissedReplies.isEmpty && mod.sheet === sheet, "sheet must not dismiss on explicit null fields")

        // 3. onpaymentauthorized returning { result: "success", data: null } and { result: "success", data: "order-123" } never crashes.
        handlerReply = #"{"result":"success","data":null}"#
        var auth1: PKPaymentAuthorizationResult?
        mod.paymentAuthorizationController(controller, didAuthorizePayment: FakePayment()) { auth1 = $0 }
        wait { auth1 != nil }
        precondition(auth1?.status == .success, "expected .success for data: null")
        precondition(sheet.result?["data"] == nil, "data: null must omit data key")

        handlerReply = #"{"result":"success","data":"order-123"}"#
        var auth2: PKPaymentAuthorizationResult?
        mod.paymentAuthorizationController(controller, didAuthorizePayment: FakePayment()) { auth2 = $0 }
        wait { auth2 != nil }
        precondition(auth2?.status == .success, "expected .success for primitive string data")
        precondition(sheet.result?["data"] as? String == "\"order-123\"", "expected JSON-quoted string data, got \(String(describing: sheet.result?["data"]))")

        print("PASS Apple Payments null and primitive handling")
    }
}
`);
    run('xcrun', ['swiftc', '-parse-as-library', '-emit-library', '-static', '-module-name', 'ExactPackage', '-emit-module',
      '-emit-module-path', join(out, 'ExactPackage.swiftmodule'), 'host/apple/modules/ExactPackage.swift', '-o', join(out, 'libExactPackage.a')]);
    run('xcrun', ['swiftc', '-parse-as-library', '-swift-version', '5', '-I', out, '-L', out, '-lExactPackage',
      'packages/payments/apple/Payments.swift', 'packages/payments/apple/PaymentButton.swift', source, '-o', join(out, 'check')]);
    const stdout = run(join(out, 'check'), []);
    expect(stdout).toContain('PASS Apple Payments null and primitive handling');
  } finally {
    rmSync(out, { recursive: true, force: true });
  }
}, 120_000);
