

### Setup
Install sqlx-cli
`cargo install sqlx-cli --no-default-features --features postgres`

### Migrations via sqlx

Run migrations
```sh
sqlx migrate run
sqlx migrate run --database-url postgres://gainzville:dev_password@localhost/gainzville_test
```


Add a new migration with the current timestamp
`sqlx migrate add <name>`

### Postgres

Start the database, -d detaches from the current shell session.
`docker-compose up -d`

Stop the database and keep volumes (container database persists between runs).
`docker-compose down`

Stop the database and erase volumes (delete container database).
`docker-compose down -v`

Connect to psql from inside the container (main database)
```sh
docker exec -it gainzville-postgres psql -U gainzville -d gainzville_dev
```

Connect to psql from inside the container (test database)
```sh
docker exec -it gainzville-postgres psql -U gainzville -d gainzville_test
```

Run pg_sandbox with test DB
`TEST_DATABASE_URL="postgres://gainzville:dev_password@localhost:5432/gainzville_test" cargo run --bin pg_sandbox`

### Sqlite

Sqlite isn't setup to use migrations or sqlx compiled queries, for now just a proof of concept.
sqlite_sandbox.rs assumes a root level test.db file.

Create tables for ./test.db
```sh
sqlite3 test.db < src/sqlite/sqlite-schema.sql
```
- There's a good chance this schema will be out of date as I evolve the PG schema!