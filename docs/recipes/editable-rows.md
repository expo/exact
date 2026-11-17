# Recipe: rows added and removed while editing

A poll's options, a recipe's steps: each row's remove is a red `minus-circle`
(`destructive=true`), shown while a row may go, and its swipe (`delete=`); Add is the section's
last row. Removing a row asks nothing; the form's Save or Cancel is the commitment.

```contract
shape Choice
  id: number
  text: string

component Options
  state options: Choice[] = [Choice(id=0, text=""), Choice(id=1, text="")]
  state nextId = 2
  action edit(id: number, v: string)
    options = options.map(o => o.id == id ? Choice(o, text=v) : o)
  action add
    options = options.concat([Choice(id=nextId, text="")])
    nextId = nextId + 1
  action remove(id: number)
    options = options.filter(o => o.id != id)
  view
    list appearance="auto" listStyle="inset-grouped" height="100%"
      section
        header
          text "Options"
        for o, i in options key=o.id
          row gap=8 align-items="center" delete=remove(o.id) disabled=(options.length <= 2)
            if options.length > 2
              button press=remove(o.id) destructive=true aria-label=`Remove option ${i + 1}` testId=`remove-${i}`
                image "symbol:minus-circle"
            input value=o.text input=edit(o.id) placeholder=`Option ${i + 1}` aria-label=`Option ${i + 1}`
              testId=`option-${i}` flex=1 min-width=0
        if options.length < 6
          button press=add testId="add-option"
            text "Add Option"
```

```contract-test
test "an option is added and removed, never below two"
  expect tree missing "remove-0"
  tap "add-option"
  type "option-2" "Tacos"
  tap "remove-0"
  expect state options.1.text == "Tacos"
  expect tree missing "option-2"
  expect tree missing "remove-0"
```

- Each row is keyed by an id it keeps (`nextId`), not its position, so a field's text and focus
  stay with their row when a row above it goes (`lower-key-position`).
- What can't be undone once saved (Delete, Close Poll, Cancel Event) asks first (`contract vocab
  confirm`).
