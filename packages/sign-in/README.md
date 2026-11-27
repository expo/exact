# Sign in with Apple and Google

Read `getIdentityCapabilities()` to decide which buttons to show. A button
press calls `credentials.get({identity})` from `@exact/passkeys`. Send the
returned credential to your server as in the [sign-in recipe](../../docs/recipes/sign-in.md).

| Host | Apple | Google |
| --- | --- | --- |
| Web | Authorization popup | Authorization popup |
| iOS and macOS | System sheet, alone or with a passkey | Unavailable |
| Android | Unavailable | Credential Manager, standalone active and passive requests |

On Android grant `auth.google <web-client-id>`. Register the package name
and signing certificate with an Android client in the same Google project.
The token is a JWT string with the web client as its audience. Pass the
server's raw nonce in `params.nonce`. Verify its value and the token on the server.

Active mode opens Google's button flow and accepts `domainHint`. Passive
mode tries authorized accounts first and retries with all accounts if
Credential Manager finds none. Set `android.filterByAuthorizedAccounts`
to choose one set explicitly. Set `android.requestVerifiedPhoneNumber`
only for passive `context: "signup"` with all accounts. Omit the filter or
set it to `false`. All three Android options take booleans.

Apple and the web always show UI. Android allows automatic selection only
for an authorized account in a passive request with `android.autoSelectEnabled: true` and optional
mediation. At sign-out, `preventSilentAccess()` clears Credential Manager's
session and requires UI across app launches. Only a successful interactive
attempt started after the latest sign-out clears that requirement. The
all-accounts retry counts as a new attempt and always shows UI.

Credential Manager supplies no selection indicator, so `isAutoSelected`
reads false. Android journals `loginHint`, `fields`, and passive
`domainHint` when supplied.

Passkeys and Google share one queue. A new ceremony supersedes a pending
passive Google or conditional passkey request. It waits behind an active
sheet. If that sheet answers while the activity is covered, a queued
passkey starts when the activity is visible again. Google starts, retries
and delivers credentials when the initiating activity resumes. Rotation
keeps the sheet open. Releasing the call or replacing, finishing or
destroying its activity aborts it and discards a late credential.

On Android the Google button uses `context` for its label. Its `text` can override
that label with `signin_with`, `signup_with`, `continue_with`, or `signin`.

A dismissal and an empty account list both return `NetworkError`.
The platform detail in `android.type` is Credential Manager's exception
type, such as `android.credentials.GetCredentialException.TYPE_USER_CANCELED`.
The app can offer passkeys separately while `google.withPublicKey` is false.
