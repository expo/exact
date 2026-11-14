# Recipe: a combobox

Filter options while the field keeps focus.

```contract
component Combobox
  state query = ""
  state open = false
  state chosen = 0
  state picked = ""
  derive hits = ["Apricot", "Apple", "Pear", "Plum"].filter(name => name.toLowerCase().includes(query.toLowerCase()))
  action edit(value: string)
    query = value
    chosen = 0
    open = true
  action began
    open = true
  action ended
    open = false
  action choose(name: string)
    picked = name
    query = name
    open = false
    chosen = 0
  action searchKey(k: string, e: KeyboardEvent)
    if not e.isComposing
      if hits.length > 0 and (k == "ArrowDown" or k == "ArrowUp" or k == "Home" or k == "End")
        preventDefault()
        let next = k == "Home" ? 0 : k == "End" ? hits.length - 1 : k == "ArrowDown" ? min(chosen + 1, hits.length - 1) : max(chosen - 1, 0)
        chosen = next
        open = true
        match hits.at(next)
          case some(name)
            scrollIntoView(`option-${name}`, block="nearest")
          case none
            chosen = 0
      else if k == "Enter" and open
        match hits.at(chosen)
          case some(name)
            preventDefault()
            choose(name)
          case none
            chosen = 0
      else if k == "Escape"
        preventDefault()
        open = false
        chosen = 0
  view
    column
      input id="query" testId="query" role="combobox" aria-label="Fruit" value=query input=edit key=searchKey focus=began blur=ended
        aria-expanded=(open and hits.length > 0) aria-controls="options"
        aria-activedescendant=(open ? (match hits.at(chosen) { case some(name) => `option-${name}`, case none => "" }) : "")
      if open and hits.length > 0
        scroll max-height="50vh"
          column id="options" role="listbox" aria-label="Fruit" testId="options"
            for name, i in hits key=name
              button id=`option-${name}` testId=`option-${name}` role="option" aria-selected=(i == chosen) tabindex=-1 retainFocus=true press=choose(name)
                text name
      text picked testId="picked"
      button "Continue"
```

```contract-test
test "filter, choose, close, and Tab out"
  tap "query"
  type "query" "ap"
  type "query" key "ArrowDown"
  expect state chosen == 1
  type "query" key "End"
  expect state chosen == 1
  type "query" key "ArrowUp"
  expect state chosen == 0
  type "query" key "Home"
  type "query" key "ArrowDown"
  type "query" key "Enter"
  expect text "picked" == "Apple"
  expect tree missing "options"
  type "query" "p"
  type "query" key "Escape"
  expect tree missing "options"
  type "query" "pl"
  expect tree has "option-Plum"
  type "query" key "Tab"
  expect state open == false
```

- Options use `tabindex=-1`; `retainFocus=true` keeps a click in the field. Tab leaves; blur closes the list.
- `aria-controls` names the listbox; `aria-activedescendant` names the active option, or is empty for none. Only the web exposes it.
- Use `next` after assigning state: actions read old state.
