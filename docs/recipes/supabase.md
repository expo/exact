# Recipe: a Supabase backend (Auth, tables, Storage, Functions)

Supabase's `postgrest-js`, `storage-js` and `functions-js` run in a data module
as published. Auth is four calls in your own `supabase.ts`, copied from `bun
exact.mjs docs supabase session` and the section after it; its `supabase(store,
now)` makes the clients.
`bun add @supabase/postgrest-js @supabase/storage-js @supabase/functions-js`.

```ts
import { session, signInAnonymously, supabase, PROJECT, SESSION } from './supabase.ts';
export const grants = `net.fetch ${PROJECT}\nsecret.keep ${SESSION}`;
// Throw a client's `error`, so a resource keeps what it showed.
const ok = <T,>({ data, error }: { data: T; error: unknown }): T => { if (error) throw error; return data; };
const sources: Sources = {
  currentSession: async ([now], store) => {
    const s = await session(store, now);
    return { userId: s?.user.id ?? '', anonymous: s?.user.is_anonymous ?? true, expiresAt: s?.expires_at ?? 0 };
  },
  signInAnonymously: async ([now], store) => { await signInAnonymously(store, now); return { ok: true, error: '' }; },
  notes: async ([user], store) => ok(await (await supabase(store, undefined, user)).db.from('notes').select('id,text').order('id')) ?? [],
  save: async ([text, user, now], store) => {
    const { db } = await supabase(store, now, user);
    const { error } = await db.from('notes').insert({ text });
    return { ok: !error, error: error?.message ?? '' };
  },
};
```

```text
  resource time = exactTime() as shape Time
  state now = 0
  resource session = currentSession() with time.epochAtZero + now as shape Session else empty(userId="", anonymous=true, expiresAt=0)
  resource notes = notes(session.userId) with session.expiresAt when session.userId != "" as shape Note[] else empty()
  mutation started as shape Reply refreshes session
  mutation saved as shape Reply queue refreshes notes
  task renew mount
    every(60000, tick)
  task anonymous when session.userId == "" and not pending(session)
    after(1, start)
  action start
    if not pending(started)
      send started = signInAnonymously(time.epochAtZero + performanceNow())
  action tick
    now = performanceNow()
  action save(text: string)
    send saved = save(text, session.userId, time.epochAtZero + performanceNow())
```

- **Sign in from a mutation.** A resource is asked again when what it reads
  changes, even mid-answer.
- **Time is an argument.** The minute tick refreshes the session while the
  app is open, and a write refreshes first; reads take `expiresAt` after
  `with`, so they ask again with the new token.
- **Writes take `session.userId`,** so a save queued before a sign-out is
  refused, not sent as the next user.
- Project setup, sign-out, Apple, the web's tokens and files over 4 MiB:
  `docs supabase accounts` and the section after it.

`supabase-js`, `auth-js` and Realtime do not run here: `docs supabase limits`.
Tested against a loopback stand-in on the web and Hermes, not a live project.
