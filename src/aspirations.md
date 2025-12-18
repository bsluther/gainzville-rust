
### General Structure
```rust
// Keep your existing architecture
pub enum Delta<T: Table> {
    Insert(T::Insert),
    Update(T::Update),
    Delete(T::Delete),
}

pub enum TableDelta {
    Actors(Delta<ActorsTable>),
    Users(Delta<UsersTable>),
}

// --- THE FIX: Boilerplate to make usage clean ---

// 1. Allow converting a specific Delta into the top-level Enum automatically
impl From<Delta<ActorsTable>> for TableDelta {
    fn from(d: Delta<ActorsTable>) -> Self {
        TableDelta::Actors(d)
    }
}

// 2. Allow converting the inner-most Insert struct directly into a generic Delta
impl From<ActorInsert> for Delta<ActorsTable> {
    fn from(insert: ActorInsert) -> Self {
        Delta::Insert(insert)
    }
}

// --- Reversible Deltas ---
pub enum Delta<T: Table> {
    // Reversible: Delete the record by ID
    Insert(T::Insert), 
    
    // Reversible: Re-insert the record
    // MUST contain the full record data to allow "undoing" the delete
    Delete(T::Snapshot), 
    
    // Reversible: Apply the 'old' values
    Update {
        pk: T::PrimaryKey,
        old: T::UpdateDiff, // The values BEFORE change (for Undo)
        new: T::UpdateDiff, // The values AFTER change (for Redo/Sync)
    },
}

// --- NEW USAGE ---

impl ActorRepository {
    pub fn insert(id: Uuid, actor_type: ActorType) -> Result<Vec<TableDelta>> {
        // Create the data payload
        let payload = ActorInsert {
            id,
            actor_type,
            created_at: chrono::Utc::now(),
        };

        // .into() handles the wrapping into Delta::Insert
        // .into() handles the wrapping into TableDelta::Actors
        Ok(vec![payload.into().into()]) 
    }
}

// The High-Level "Why"
#[derive(Serialize, Deserialize, Debug)]
pub enum DomainIntent {
    UserChangePassword,
    UserPromoteAdmin { reason: String },
    WorkoutComplete { duration_sec: u32 },
    // "Anonymous" changes (generic edits)
    ManualEdit, 
}

// The "Commit" Object
pub struct Mutation {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub intent: DomainIntent, // Replaces 'name: String'
    pub changes: Vec<TableDelta>, // The atoms
}

impl User {
    // Pure function: Returns a Mutation description, doesn't touch DB yet
    pub fn change_password(
        &self, 
        current_hash: &str, 
        new_hash: &str
    ) -> Mutation {
        
        // 1. Construct the low-level atom (The Delta)
        let delta = Delta::<UsersTable>::Update {
            pk: self.id,
            // Capture old state for UNDO
            old: UserUpdate { password_hash: Some(current_hash.to_string()), ..Default::default() },
            // Capture new state for REDO / SYNC
            new: UserUpdate { password_hash: Some(new_hash.to_string()), ..Default::default() },
        };

        // 2. Wrap it in the high-level Intent (The Mutation)
        Mutation {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            intent: DomainIntent::UserChangePassword, 
            changes: vec![delta.into()], // Assuming Into<TableDelta> is implemented
        }
    }
}

fn apply_mutation(db: &mut Database, mutation: Mutation) -> Result<()> {
    // Start DB Transaction
    let tx = db.begin_transaction()?;

    for change in mutation.changes {
        match change {
            TableDelta::Users(delta) => apply_user_delta(&tx, delta)?,
            TableDelta::Actors(delta) => apply_actor_delta(&tx, delta)?,
        }
    }

    // Commit
    tx.commit()?;
    
    // Post-Commit Hooks
    // Because we have the high-level 'intent', we can do side effects here!
    match mutation.intent {
        DomainIntent::UserChangePassword => send_email("Password Changed"),
        DomainIntent::UserPromoteAdmin { .. } => send_slack_alert("New Admin!"),
        _ => {},
    }

    Ok(())
}
```


### Trait Repos where impls are created in a transcation and hold tx as state
```rust
// The Repo lives only as long as the Transaction borrow
pub struct PgAuthnRepo<'a> {
    tx: &'a mut sqlx::Transaction<'a, sqlx::Postgres>,
}

impl<'a> AuthnRepo for PgAuthnRepo<'a> {
    async fn is_email_registered(&self, email: Email) -> Result<bool> {
        // Use self.tx directly
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE email = $1")
            .bind(email.as_str())
            .fetch_one(&mut **self.tx) // Deref magic to get the Executor
            .await?;
            
        Ok(count > 0)
    }
}
```