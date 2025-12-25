use uuid::Uuid;

#[derive(Debug)]
struct Activity {
    id: Uuid,
    owner_id: Uuid,
    source_activity_id: Option<Uuid>,
    name: String,
    description: Option<String>,
}

#[derive(Debug)]
pub struct ActivityPatch {
    owner_id: Option<Uuid>,
    source_activity_id: Option<Option<Uuid>>,
    name: Option<String>,
    description: Option<Option<String>>,
}
