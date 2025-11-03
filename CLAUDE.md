axismundi is a constructed language creation and documentation app. users will use it from
the web (or other clients, maybe) to construct languages

always run tests using `just test`; we need this to bring up the db
example invocations:
- `just test`: runs full test suite, no coverage
- `just test test_get_user_not_found`: runs just the test named that
- `just cov`: runs full test suite with coverage
- `just test test_get_user_not_found 1`: runs just the test named that with coverage