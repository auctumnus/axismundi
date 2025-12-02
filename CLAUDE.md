all sql is in postgres 18

when you write sql, you should consult the tables @migrations

always use `just test` for tests. if you need to do something more complicated than what i have in the justfile you should ask about it

sometimes the db explodes and i get a bunch of errors in my ide and i think you get told about them but you can ignore the sqlx failing to connect to database queries its fine

writing code off to the side is pawbably not a reasonable debugging strategy

axum uses {id} style route parameters, not :id style

html for pages goes in templates, code for them goes into src/controllers/html

when you need to test changes to the html controllers, don't bother running `just test`, do `just build`

"secure routes" means "has user security implications", not "needs authentication"

the frontend has typescript stored in `frontend/src`; that's also where the css and such are