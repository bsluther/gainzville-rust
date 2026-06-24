use uuid::Uuid;

enum DataType {
    Float,
    Int,
    String,
    Timestamp,
}

struct Column {
    name: String,
    data_type: DataType,
}

struct Relation {
    pub schema: Vec<Column>,
}

enum AttributeType {
    Text,
    Numeric,
}

enum SelectAttributeOperator {
    In,
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

struct SelectAttributeFilter {
    attr_id: Uuid,
    // parameter: Expr,
}

// ---------- DOMAIN SURFACE ---------- //

enum Filter {
    Activity(ActivityFilter),
    Entry,
    Attribute,
}

struct ActivityFilter {
    include: Vec<Uuid>,
}
struct TBD {}
struct EntryFilter {
    field: EntryFieldRef,
    predicate: TBD, // Do we generalize to an expression? Assume a unary fn which takes the referenced field as arugment? Something else?
}

enum FieldRef {
    Entry(EntryFieldRef),
    Attribute(AttributeFieldRef),
}

enum EntryFieldRef {
    Activity,
    IsComplete,
    IsSequence,
    // If entry has no canonical instant, derive from ancestors.
    DerivedCanonicalInstant,
    Start,
    End,
    Duration,
}

enum RangeFold {
    Min,
    Max,
    Mean,
    Omit,
}

enum Facet {
    Plan,
    Actual,
}

struct AttributeFieldRef {
    attribute_id: Uuid,
    facet: Facet,
    range_fold: RangeFold,
}

struct Catalog {}

fn resolve_type(field: FieldRef, catalog: Catalog) -> DataType {
    todo!()
}
