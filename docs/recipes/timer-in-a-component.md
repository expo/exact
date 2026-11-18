# Recipe: a component that needs a timer

A child at a bounded use can own a task and the state that gates it. Each
instance starts and stops its own timer ([timers](timers.md) has the schedules):

```contract
component App
  view
    column
      Capsule(id="left")
      Capsule(id="right")

component Capsule
  props
    id: string
  state charge = 0
  state holding = false
  task charging when holding and charge < 100
    every(16, step)
  action step
    charge = min(100, charge + 2)
  action hold(on: bool)
    holding = on
  view
    column
      button pointerdown=hold(true) pointerup=hold(false) testId=id
        text (charge < 100 ? "Hold to charge" : "Charged")
      progress value=charge max=100
```

The task's action runs in its instance: charging the left capsule does not
change the right one. Destroying a capsule stops its task. A hidden tab can
remain alive; gate its work on activity passed from the router. `key=` restarts
a timer when its key changes, as at the root. Reload restarts timers over
compatible carried state.

A task cannot belong to a component under a data-sized `for` or in a virtualized
list. Put shared timing in the enclosing screen and pass its value or action to
those rows. `every(frame, …)` is allowed at bounded uses; each armed instance
runs an action per frame.
