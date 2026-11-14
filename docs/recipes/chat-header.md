# Recipe: a chat header with an avatar, and day pills

The iOS bar holds a title and buttons, not a photo: a chat's header is its own row.

```contract
routes nav
  home "/"

shape Message
  id: string
  body: string
  at: number
  newDay: bool

shape Time
  epochAtZero: number
  utcOffset: number

component Chat
  resource thread = loadThread() as shape Message[] else empty()
  resource time = exactTime() as shape Time
  view
    main navigationKey=`${top(nav).id}` width="100%" height="100%"
      for e in stack(nav) key=e.id
        column navigationKey=`${e.id}` navigationBar="none" navigationScroll="transcript" position="absolute" inset=0
          row align-items="center" gap=10 padding="8px 12px" border-bottom="1px solid -exact-separator"
            image "symbol:person" aria-hidden="true" font="-exact-title1"
            column flex=1 min-width=0
              text "Ada" role="heading" aria-level=1 font="-exact-headline" testId="name"
              text "online" font="-exact-footnote" color="-exact-secondary-label"
            button aria-label="Call"
              image "symbol:phone"
          list id="transcript" appearance="none" virtualized=true scroll-start="end" flex=1 min-height=0
            for m in thread key=m.id
              column padding="3px 12px"
                if m.newDay
                  text formatDay(m.at, time.epochAtZero, time.utcOffset, "day") align-self="center"
                    padding="2px 10px" border-radius=10 background-color="-exact-fill" font="-exact-caption1"
                    testId=`day-${m.id}`
                text m.body white-space="pre-wrap" testId=`message-${m.id}`
```

```ts
import type { Answer, Result, Sources } from './app.contract.d.ts';

export const grants = '';

const day = 86400000, start = Date.UTC(2026, 0, 1);
const messages: Result<'loadThread'> = [
  { id: '1', body: 'See you', at: start - day, newDay: true },
  { id: '2', body: 'At noon', at: start, newDay: true },
];
const sources: Sources = { loadThread: () => messages };
export const answer: Answer = (source, args, store, storage, native) => sources[source](args, store, storage, native);
```

```contract-test
test "the header names the chat; each day opens with its pill"
  clock data
  expect text "name" == "Ada"
  expect text "day-1" == "Yesterday"
  expect text "day-2" == "Today"
```

- `navigationBar="none"` on the route's element draws no bar, so the row is the header. A
  pushed route then has no Back: put a `button press=back` first.
- A photo: `image "assets/ada.jpg"`, fixed `width` and `height`, `border-radius` half of it.
- The pill is the row root's first child, never its sibling ([long lists](long-lists.md)).
  `formatDay` names the day in the person's locale; tick `now` as the
  [dates recipe](dates-and-days.md) does to pass midnight.
- A time on a bubble's last line: in a `box position="relative"`, end the text with a
  hidden run, `` text `  ${m.time}` opacity=0 aria-hidden="true" ``, and draw the time
  `position="absolute" right=8 bottom=4`. A full last line pushes it down.
