# Android notification groups

Use `android.group` on each child and `android.groupSummary=true` on a
separate post for the summary. The group key is a non-empty string. A channel
sets importance and sound. On Apple, `apple.threadIdentifier` groups posts
without an Android summary.

```contract
  action fern
    showNotification(title="Fern", tag="fern", android.group="field-trip")
    showNotification(title="Field trip", body="1 specimen", tag="trip-summary", android.group="field-trip", android.groupSummary=true)
```

When Moss arrives, post it under its own tag and replace `trip-summary` with
the new count. Close each child by tag. After the last child goes, call
`closeNotification(tag="trip-summary")`. Closing the summary while children
are visible dismisses them too. The app supplies any navigation on the summary.

On API 35 and 36 AndroidX lets both children and summaries alert by default.
Exact posts summaries with `GROUP_ALERT_CHILDREN` so updating the summary
adds no second alert. A silent child uses `GROUP_ALERT_SUMMARY` and keeps its
named group. Without a group, Exact passes AndroidX’s fallback key, `silent`.
Use another key for an app-owned bundle. API 35 groups posts under `silent`,
while API 36 converts that key to a silent flag and clears the app group.
[Android notification groups](https://developer.android.com/develop/ui/views/notifications/group)
