all sql is in postgres 18

when you write sql, you should consult the tables @migrations

always use `just test` for tests. if you need to do something more complicated than what i have in the justfile you should ask about it

sometimes the db explodes and i get a bunch of errors in my ide and i think you get told about them but you can ignore the sqlx failing to connect to database queries its fine

if the linker explodes, you can `rm -r target` (be kind of careful with this). there's some incremental compilation bug going on

writing code off to the side is pawbably not a reasonable debugging strategy

axum uses {id} style route parameters, not :id style

html for pages goes in templates, code for them goes into src/controllers/html

when you need to test changes to the html controllers, don't bother running `just test`, do `just build`

"secure routes" means "has user security implications", not "needs authentication"

the frontend has typescript stored in `frontend/src`; that's also where the css and such are


## Debugging Strategies

### Check What SQL is Generated

SQLx macros print SQL at compile time. Check compiler output to see the exact queries.

### Use Print Debugging

Standard Rust debugging with `println!` or `dbg!` works:

```rust
println!("User: {:?}", user);
dbg!(&quotation);
```

### Test Database State

Connect to the test database during `just test`:

```bash
# In another terminal while tests are running
docker exec -it axismundi-postgres-test psql -U postgres -d axismundi
```

### Check Frontend Console

Open browser DevTools console to see JavaScript errors and `console.log` output.

### Read Error Messages Carefully

AppError messages are designed to be helpful. The error contains:
- Human-readable message
- HTTP status code
- Validation errors (if applicable)

## When in Doubt

1. **Look at similar code** - Find an existing feature that's similar to what you're building
2. **Check migrations** - Use `@migrations` to see exact table structures
3. **Run tests** - `just test` catches many issues
4. **Build incrementally** - Test each piece before moving on
5. **Ask the user** - If requirements are unclear, ask rather than guessing

## Architecture Principles

This codebase follows these patterns:

1. **Repository pattern** - All database access goes through repositories
2. **Separation of concerns** - API vs HTML controllers are separate
3. **Validation at the edge** - Validate input as early as possible
4. **Type safety** - Compile-time checked SQL queries, strong typing throughout
5. **Simplicity** - Avoid over-engineering, keep things straightforward

Following these patterns will make your code fit naturally into the existing codebase.

## Template patterns

generally:
- headings and titles are lowercase
- full buttons are Title Case, smaller buttons (e.g. in a header-with-actions) are lowercase
- 

### Lists

there are 2 kinds of lists: lists on search pages, and preview lists. preview lists are introduced by a `header-with-actions`
which has a [+ new $RESOURCE ] action and a [= view all ] action, then have 3 cards. search lists are
introduced by a search bar and have pagination after them

### Cards

resources are represented by cards. there are generally 3 patterns to cards: either it's a clickable card
(points to a full resource page), or it's a card with actions (has an edit and delete button usually), or
it's a full card (on its own resource page). clickable cards and cards with actions appear in lists, and
can be referred to as "preview cards"

#### Preview cards

preview cards have a structure of information on the left and actions on the right. the information on
the left is 3 lines: the main identifying piece of information (a user's name, a word's normal form),
a summary (the first line of a language's description, the word's first definition), and a "created by / at"
section. the right side will either have a like button, edit/delete actions, or nothing

#### Full cards

full cards are a bit more idiosyncratic and ill avoid noting too many general patterns. the language card is a good reference

### User badges

user badges can be small, medium, or large. they are either clickable or not


