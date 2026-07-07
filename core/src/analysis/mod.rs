struct FieldDescription {}

enum FieldValue {}

struct Relation {
    schema: Vec<FieldDescription>,
    tuples: Vec<Vec<FieldValue>>,
}

struct View {
    segments: Vec<Segment>,
}

struct Segment {
    filter: Vec<Filter>,
    group_by: Vec<GroupBy>,
    aggregate: Vec<Aggregate>,
    order_by: Vec<OrderBy>,
}

enum Filter {
    String,          //
    LiteralEquality, // expr == literal
}
// LHS's: time, number, actvity, attr_value
struct FilterPredicate {
    lhs: FilterComparable,
    op: CompareOp,
    rhs: Literal,
}

enum CompareOp {
    Eq,
    Lt,
    Lte,
    Gt,
    Gte,
    In,
}

enum Literal {
    AttributeValue,
    String,
    Number,
    Timestamp,
}

struct GroupBy {}

struct Aggregate {}

struct OrderBy {}

/// LHS of a filter operation.
enum FilterComparable {
    FieldReference,
    TimeTruncation,
}
trait Expression {}

struct FieldReference {}

struct TimeTruncation {}
