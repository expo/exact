# Recipe: a long list or grid that loads more

Virtualize rows, each holding N cards (two here). The list has one direct keyed
`for`; its template has one root in normal flow. Put conditions inside that root.

```contract
shape Product
  id: string
  title: string
shape GridRow
  id: string
  cards: Product[]
shape Catalog
  rows: GridRow[]
  hasNext: bool
shape Reply
  ok: bool

component Shop
  resource catalog = loadCatalog() as shape Catalog else empty()
  mutation more as shape Reply refreshes catalog
  derive loading = pending(catalog) or pending(more)
  derive rows = loading ? catalog.rows.concat([GridRow(id="loading", cards=[])]) : catalog.rows
  action loadMore
    if catalog.hasNext and not loading
      send more = nextPage()
  view
    list id="products" testId="products" virtualized=true appearance="none"
      height="100%" estimated-item-height=124 reachend=loadMore reach-margin="100%"
      for r in rows key=r.id
        row gap=12 padding=12
          if r.id == "loading"
            text "Loading…" role="status" testId="loading"
          else
            for p in r.cards key=p.id
              column flex=1 min-width=0 min-height=100
                text p.title testId=`product-${p.id}`
```

In `app.ts`, append the page before `refreshes` re-reads the resource's answer
([after a write](after-a-write.md)). This fake API has three pages of twenty;
replace `fetchPage` with your API and its next cursor, and name its origin in `grants`:

```ts
import type { Answer, Result, Sources } from './app.contract.d.ts';
export const grants = '';
type Product = Result<'loadCatalog'>['rows'][number]['cards'][number];
let items: Product[] = [];
async function fetchPage(page: number) {
  return { items: Array.from({ length: 20 }, (_, i) => {
    const id = String(page * 20 + i + 1);
    return { id, title: `Product ${id}` };
  }), next: page < 2 ? page + 1 : null };
}
let cursor: number | null = 0;
async function append() {
  if (cursor === null) return;
  const page = await fetchPage(cursor);
  items = items.concat(page.items);
  cursor = page.next;
}
const sources: Sources = {
  loadCatalog: async () => {
    if (cursor === 0) await append();
    return { hasNext: cursor !== null, rows: items.filter((_, i) => i % 2 === 0)
      .map((p, i) => ({ id: p.id, cards: items.slice(i * 2, i * 2 + 2) })) };
  },
  nextPage: async () => { await append(); return { ok: true }; },
};
export const answer: Answer = (source, args, store, storage, native) => sources[source](args, store, storage, native);
```

- The loading row is last in the collection while a page is out, never a sibling
  of the `for`. `hasNext` stops requests after the last page; the pending guard
  prevents duplicates. A failed mutation leaves the previous answer intact.
- `reach-margin` is a nonnegative px or % of the list's port (default one port,
  `"100%"`); increase it to prefetch earlier. A fling can extend the lead. A first
  page shorter than two ports loads the next on first paint.
