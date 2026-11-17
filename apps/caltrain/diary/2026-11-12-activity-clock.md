surfaces: ios
agent: Claude, Opus 5.5
task: make the Live Activity count down on the real clock

### Rough
The app's schedule runs on a seeded day, and the activity carried those
instants to the system, which draws on the real clock: the countdown ended
in August 2026 and read 0:00. Nothing in the app's own drive shows it,
because the agent's clock and the seeded one agree.

### Lean in
The watch entry had already solved it: `exactTime()`'s `epochAtZero` less
the seeded epoch moves an instant from one clock to the other. The phone's
app reads the same resource and adds that offset where it builds the
activities' states.

### Needed
Nothing new in exact2.

### Checkpoints
The activity tests set the runner's time to 2027-01-15 and require the
departure at ten minutes after it (twenty once delayed) and the alert
updated at it; both failed before the change. Caltrain's ten app tests pass.
Not run: a Live Activity on a phone or simulator.
