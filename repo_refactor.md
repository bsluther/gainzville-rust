# Repository Refactoring Design

## Problem
Currently, repository implementations (`SqliteContext`, `PgContext`) hold a mutable reference to a `Transaction`. This couples the repository instance to the transaction lifecycle, requiring a new repository instance to be created for every transaction. This makes the architecture somewhat rigid and "awkward" to work with.

## Solution: Stateless Repositories with Generic Executors

The standard pattern to solve this in `sqlx` is to decouple the **Repository** (logic) from the **Unit of Work** (transaction).

1.  **Repositories** become stateless structs (or hold only static config).
2.  **Transactions** (or Pools) are passed as arguments to the repository methods.
3.  **Traits** become generic over the `sqlx::Database` type.

### 1. Update Core Traits (`core/src/repos.rs`)

Update the traits to be generic over `DB: sqlx::Database`. The methods should accept an `executor` argument. Using `impl Executor` allows the method to accept both `&Pool` (for simple reads) and `&mut Transaction` (for atomic operations).

```rust
use sqlx::{Database, Executor};

pub trait AuthnRepo<DB: Database> {
    // 'e is the lifetime of the executor reference
    async fn is_email_registered<'e, E>(&self, executor: E, email: Email) -> Result<bool>
    where
        E: Executor<'e, Database = DB>;

    async fn find_user_by_id<'e, E>(&self, executor: E, actor_id: Uuid) -> Result<Option<User>>
    where
        E: Executor<'e, Database = DB>;
        
    // ... other methods
}
```

### 2. Implement Stateless Repositories (`sqlite/repos.rs`)

The repository structs no longer need to hold a transaction lifetime. They can be unit structs.

```rust
pub struct SqliteAuthnRepo;

impl AuthnRepo<Sqlite> for SqliteAuthnRepo {
    async fn is_email_registered<'e, E>(&self, executor: E, email: Email) -> Result<bool>
    where
        E: Executor<'e, Database = Sqlite>
    {
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE email = ?")
            .bind(email.as_str())
            .fetch_one(executor) // executor passed directly to sqlx
            .await?;
        Ok(count > 0)
    }
    
    // ... implementation for other methods
}
```

### 3. Update Service Layer (`core/src/actions.rs`)

The `ActionService` methods currently take `impl AuthnRepo`. They should be updated to take the repository (logic) and the transaction (state) separately.

```rust
use sqlx::{Database, Transaction};

impl ActionService {
    pub async fn create_user<DB: Database>(
        repo: &impl AuthnRepo<DB>,    // The stateless logic
        tx: &mut Transaction<'_, DB>, // The active transaction
        action: CreateUser
    ) -> Result<Mutation> {
        
        // Pass the transaction (which implements Executor) to the repo
        // &mut **tx dereferences the Transaction wrapper to the underlying connection
        if repo.is_email_registered(&mut **tx, action.user.email.clone()).await? {
            return Err(DomainError::EmailAlreadyExists);
        }

        // ... rest of logic
    }
}
```

### 4. Update Controllers (`sqlite/controller.rs`)

In the controller, you instantiate the repo once (since it's stateless) and pass the transaction explicitly.

```rust
pub async fn run_action<'a>(&'a self, action: Action) -> Result<Transaction<'a, Sqlite>> {
    let mut tx = self.pool.begin().await?;
    
    // Repos are now cheap/free to create. 
    // Could also be stored on the Controller struct itself if they remain stateless.
    let authn_repo = SqliteAuthnRepo; 

    let mx = match action {
        Action::CreateUser(action) => {
            // Pass both repo and tx
            ActionService::create_user(&authn_repo, &mut tx, action).await?
        }
        // ...
    };

    // ... apply deltas
}
```

### Benefits
- **Decoupling**: Logic is separated from the database connection lifecycle.
- **Flexibility**: You can run queries on a Pool without starting a transaction if needed (e.g., for simple reads in the UI).
- **Type Safety**: The `DB` generic ensures you can't pass a Postgres transaction to a Sqlite repo.