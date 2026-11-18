// The parameters a drive opens its page with (`agentPage` in scripts/agent-launch.mjs): the carrier's, not the app's.
// The router keeps them on every History entry it writes under a drive and leaves them out of the app's launch route
// (LLP 1038), and the dev loop's reload restores them (host/web-js/dev.mjs), so a reload keeps the drive as it launched.
export const agentParameters = [
  "agent",
  "seed",
  "locale",
  "formatLocale",
  "timeZone",
  "epoch",
  "storage",
  "failFetch",
  "authFixture",
  "hatches"
];
