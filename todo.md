- [ ] Move validation errors into dedicated error type.
- [ ] Write a generic apply function for any model (one for Pg, one for Sqlite).
- [ ] There is going to be an issue when I want to run a test which runs multiple actions. The
execute_action function on the controllers returns a tx but doesn't take one as an argument. Will
need to pass the tx as an argument to be able to rollback the transaction, which allows for
parallel tests because it isolated each test from each other through the transaction boundary and
never commits.
    - Is it as easy creating an alternate constructor which takes tx as an arg instead of pool?