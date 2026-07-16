#![allow(unused)]
use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use crate::models::attribute::{LengthMeasurement, LengthUnit, MassMeasurement, MassUnit};

// QUESTIONS/THOUGHTS/ETC
//
// For a while I've been saying the Spec doesn't contain any derived information, because it
// depends on the user's catalog and can get stale. But I could also just store the stale spec
// and re-run everything when the spec is loaded to get the latest. Why would you want that? So
// type information is unified into a single place. That started to sound appealing when
// designing the type-level data structures. But even as I typed that, the word "re-run" made the
// downside obvious: what's the *input* to whatever it is that re-runs? That's the catalog-indep-
// endent part, and now you have to pick it apart from all the derived bits.
// Settled: "re-run" on what reveals the problem.
//
// Is every row always an Entry? Eg does it always have an entry column?
// No. Once we aggregate, we're not looking at entries.
// For Position ops like FirstAncestorWhere, that suggests an explicit Entry column.
// Settled: explicit Entry column.
//
// Expressions/values across the 3 levels:
//     Spec               Type             Value
//     Expr               Ty               Cell
//     authored           derived          executed

// GLOSSARY
//
// Expression:
// An spec-time expression, leaves are references or literals. Composite expressions arise from
// unary operators, binary operators, transforms, entry accessors, and the odd-man-out,
// NearAncestorWhere. Naming: <Operator>Expr says an expression involving an operator.
//
// Scope / execution context:
// Scope is the type-level thing, a set of variables that some expressions have access to. Execution
// context is the execution time equivalent, the values of the variables. The names of those
// variables exist in the namespace corresponding to the scope.
//
// Environment:
// The execution-time counterpart of a scope.
//
// Namespace:
//
// Binder:
// Any spec node that mints a slot.
//
// Slot:
// A column in a scope. At the spec-level namespaces are implicit, the type level  must resolve them
// to explicit sets to resolve types. At execution-time, where we go look for a cell.
//
// First-order serialized IR:
// Spec is a first-order serialied IR because it's made up of plain data, no functions or closures.
// Lambda-like things are encoded as binder + body (eg NearestAncestorWhere) to keep everything
// serializable and machine-readable.
//
// Segment:
// At present, one reduce step:
// filter -> groups -> aggregates -> having.
// Consumes it's input schema and mints its output schema: key slots + aggregate output slots.
//
// Resolve:
// Transform Spec -> Ty. Not sure if this is standard lingo, should check. This is what compilers
// do all the time, determine the type of expressions.
//
// Eval/Evalute:
// Transforms (Spec, Ty, Env) -> Cell | Relation
// Evaluate an expression, operator, definition, spec, etc. to produce the result in some context.
//
// Def:
// Suffix for a definition sites: structs whose job it is to mint slots and bind an expression or
// fold to them.
//
// Input scope:
// Perhaps used too loosely, this roughly means either "the scope of the input to an expression or
// operator" or "the source scope at the stat of a segment, the input rows".
//
//
//=========================================================//
//                        SPEC                             //
//                    (persistent)                         //
//=========================================================//

struct SlotId(Uuid);
impl SlotId {
    pub fn new() -> Self {
        SlotId(Uuid::new_v4())
    }
}

/// A pipeline from a base source to a series of segments.
/// Something like a Query in Malloy.
/// Something like a query plan in a database execution engine.
struct PipelineSpec {
    source: BaseSource,
    segments: Vec<Segment>,
}

// Right now there's one base source: all the user's completed, non-template entries.
// This could extend to defining other sources via pipelines for composition.
enum BaseSource {
    Entries(EntriesSource), // owner-scoped, !is_template, is_complete
}

/// The cannonical base source: all of a user's completed, non-template entries.
struct EntriesSource {
    slot_id: SlotId,
}

/// One segment in a pipeline. Each phase has it's own scope (set of slots) where expressions are
/// evaluated, and some expressions (NearestAncestorWhere) create a temporary evaluation scope.
///
/// Projection is not yet supported. Malloy splits Segment :: Reduce | Project, that's looks like
/// the direction we'll go.
struct Segment {
    /// Filter Exprs must resolve to booleans.
    filters: Vec<Expr>,
    /// A GroupBy just partitions by some Expr (the group key) evaluated in the input scope, the
    /// group key becomes a slot in the group's output row.
    /// Multiple GroupBy's mean we group by the composite key. E.g. group_by: [x, y] yields a
    /// composite group key (x,y) with each group consisting of rows where (x,y) is equal; typical
    /// SQL semantics.
    group_by: Vec<OutputDef>,
    aggregates: Vec<AggregateDef>,
    /// A post-aggregation filter which is scoped to the slots defined in group_by + aggregate.
    having: Vec<Expr>,
}

// Define a new slot and bind the provided `Expr` to it.
// Naming: why is this called a Def rather than a Spec?
struct OutputDef {
    slot_id: SlotId,
    label: Option<String>,
    expr: Expr,
}

//============== Operator =============//

enum UnaryOp {
    Not,
}

/// A binary operator of type (A, B) -> O, both inputs are evaluted in the same scope.
/// - N.b. In has type (T, List<T>) -> Bool
enum BinaryOp {
    Eq,
    Lt,
    And,
    In, // activity IN [PushUp, BenchPress, OverheadPress]
        // When categories land:
        // IsSubcategoryOf
}

//============== Transform =============//

/// A Transform is an expression where the resolved Type of the output depends on the *payload* of
/// the transform. Whereas an Operator's output type depends only the input *type*, eg
/// Add<T> :: (T, T) -> T
/// to resolve a transform we have to look at the expression node's data (not the row, just the
/// parameters of the TimeTruncation, which is fixed at spec-time), eg
/// TimeTruncation(unit) :: Timestamp -> TimeBucket { unit }
enum Transform {
    TimeTruncation(TimeUnit),
    // Bin
    // Band
}

enum TimeUnit {
    Day,
    Week,
    Month,
    Year,
}

//============== Expression =============//

/// The workhorse: an expression, with leaves = Literal | SlotRef.
/// Note that aggregations are not in here
enum Expr {
    Literal(LiteralSpec),
    // ↓ A slot isn't exactly a column, it can exist in another scope (although it is a column in that scope...)
    // A reference to a slot (column) in the current source. The primary way of
    // referencing values.
    // Slot has been a useful non-conflicting word, but is Column a better word?
    SlotRef(SlotId),
    Unary(UnaryExpr),
    Binary(BinaryExpr),
    Transform(TransformExpr),
    EntryAccessor(EntryAccessorExpr),
    NearestAncestorWhere(NearestAncestorWhere),
}

struct UnaryExpr {
    op: UnaryOp,
    operand: Box<Expr>,
}

struct BinaryExpr {
    lhs: Box<Expr>,
    op: BinaryOp,
    rhs: Box<Expr>,
}

struct TransformExpr {
    transform: Transform,
    operand: Box<Expr>,
}

struct EntryAccessorExpr {
    accessor: EntryAccessor,
    operand: Box<Expr>,
}
enum EntryAccessor {
    Instant,
    Start,
    End,
    Duration,
    IsSequence,
    IsComplete,
    Activity,
    Attribute {
        attr_id: Uuid,
        aspect: Option<Aspect>,
        range_fold: Option<RangeFold>,
    },
}

enum Aspect {
    Plan,
    Actual,
}
enum RangeFold {
    Min,
    Max,
    Mean,
}

/// NearestAncestorWhere :: (Entry, SlotId, Predicate) -> Entry
/// Maps an entry to the first ancestor that satisfies a predicate.
///
/// This is the operator that drove the expansion of the segment scope: we need to evaluate the
/// predicate on a different set of rows (ancestor candidates) than the outer scope. Decision was to
/// expand scope from one-per-Segment to a tree of scopes per-Segment. Passing the `binds` SlotId to
/// the predicate Expr is the same idea as passing a SlotId to an OutputDef, in reverse: for the
/// OutputDef we tell it where to write its results to, for the predicate we tell it where to read
/// candidate rows into the Expr.
///
/// Example: climb_sessions = GROUPBY NearestAncestorWhere(activity == ClimbingSession)
struct NearestAncestorWhere {
    // Must resolve to an Entry.
    // Evaluated in the outer scope.
    operand: Box<Expr>,
    // The predicate expression is essentially a lambda, it needs to be resolved on all the
    // ancestors of a row, a *different* set of rows than the current execution context. Create a
    // new execution context - a new slot
    binds: SlotId,
    // Must resolve to a Boolean.
    // Evaluted in the new, inner scope; scope = { binds }
    predicate: Box<Expr>,
}

//============== Aggregation =============//

/// Aggregates are *decomposed*: write the folds into scalar slots, combine them in a standard
/// expression over those slots. Also called Let-binding.
///
/// We considered two axes in the design:
/// (1) How to represent aggregate expressions: inline vs decomposed.
/// (2) Where do fold bindings live?
///
/// (1) Prior-art research agent found two approaches:
///  - Inlining: sum(x) / sum(y), primarily used by human-typed surfaces (eg SQL) where you write
///    queries by hand.
///    These systems still decompose - they just do it a later stage, because they're translating
///    SQL into a query plan. In other words, it's just a matter of *when* and *who* does the
///    decomposition; GV opts to skip the de-sugaring step.
///  - Decomposition: evaluate each aggregate, write the outputs to slots, then compose an expression
///    out of the results, eg SlotRef(b7) / SlotRef(a2). Primarily used in machine-authored systems,
///    like ours: we use a builder to construct the actual syntax, no one types out SlotRef(b7).
///  - We chose decomposition, which is aligned with prior art.
///
/// (2) The characteristic of GV that drove this decision is that GV let's users persist and edit
///     an AST. Most systems generate an AST from SQL or some other query language, in GV we present
///     a UI builder to view and edit the AST directly. Approaches (a) and (b) below both use
///     shared references to bindings (slots), but those systems don't have to worry about what
///     happens when a shared reference is edited or deleted, which could leave a reference dangling
///     or invalid.
///
///     Prior-art research agent found two approaches, we used a third:
///     (a) Stage-level, internal pool. Aggregate results are written into a private pool inside
///         an AST aggregation node, readable only by that stages output expressions.
///         Query engines (eg Postgres, Spark) go this way.
///     (b) Stage-level, public output columns. Aggregate results become real output columns which
///         are accessed in a later, downstream stage by expressions.
///         Pipeline forms (eg Druid, Mongo, Vega-Lite, Elastisearch) go this way.
///     (c) Per-output, private. Aggregate slots are *local* to the AggregateDef.
///         One system, DAX, used this approach, and that system persists and edits per-definition
///         formulas, similar to GV.
///
///     (a) is motivated by optimization, this allows them to de-dup repeated expressions.
///     (b) doesn't have any other choice, a pipeline-stage can't express the results in any other
///         way.
///
///     We make the simplifying move of keeping the slots local at the cost of re-computing
///     duplicate sub-expressins.
///     Common subexpression elimination (CSE) is one optimization we could use to avoid the
///     duplicate work.
///
/// Considered: a separate AggExpr { Aggregation, Unary, Binary, Literal, ... } without explicit
/// fold slots, but it forces duplicating every Expr to express post-aggregation expressions, even
/// as Expr grows to include more variants.
struct AggregateDef {
    /// The folds, each minting a slot.
    aggs: Vec<AggregateBinding>,
    /// The slot we write the combining expr into. A HAVING clause would read this, otherwise it is
    /// part of the output of the segment.
    slot_id: SlotId,
    label: Option<String>,
    /// The combining expr, scoped to the fold slots, eg SlotRef(b7) / SlotRef(a2).
    /// Has access to all the slots that `aggs` minted + the GroupBy's key slots.
    expr: Expr,
}

struct AggregateBinding {
    slot_id: SlotId,
    aggregation: Aggregation,
}

/// Examples:
///
/// sum(x + y) where { tag == "good" } has parts
///   function: sum
///   filter: tag == "good"
///   operand: x + y
///
/// count() where { reps > 10 } has parts
///   function: count
///   operand: None
///   filter: reps > 10
struct Aggregation {
    function: AggFunction,
    /// The pre-aggregation filter, evaluated per-row in the input scope. This filters over the rows
    /// as-is, *not* over the operand expression, which allows you to say something like
    /// `count() where { reps > 10 }`.
    filter: Option<Expr>,
    /// The pre-aggregatioon expression, evaluated in the input scope.
    /// None is legal only for Count, counts rows; enforced during resolution.
    /// Some(expr) for a Count means count only present values.
    operand: Option<Expr>,
}

enum AggFunction {
    Count, // nullary
    Sum,
    Avg,
    Min,
    Max,
}

enum LiteralSpec {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Mass {
        value: f64,
        unit: MassUnit,
    },
    Length {
        value: f64,
        unit: LengthUnit,
    },
    /// A date without time or timezone information, timezone comes from spec context.
    /// instant >= date should be (start_of_day(date), infinity)
    /// whereas
    /// instant <= date should be (-infinity, start_day(date + 1 day))
    /// Implication: we can't just collapse to timestamp, interpretation is contextual to the
    /// operator.
    Date(chrono::NaiveDate),
    RelativeTime(RelativeTime),
    /// Durations are formatted cannonically rather than specifying by-unit.
    DurationMs(u64),
    Activity(Uuid),
    /// Homogeneity checked at resolve-time.
    List(Vec<LiteralSpec>),
}

struct RelativeTime {
    anchor: TimeAnchor,
    offset: (i64, TimeUnit),
}
enum TimeAnchor {
    Now,
    StartOf(TimeUnit),
}

fn sandbox(act_id: Uuid) {
    let expr = BinaryExpr {
        lhs: Box::new(Expr::EntryAccessor(EntryAccessorExpr {
            accessor: EntryAccessor::Activity,
            operand: Box::new(Expr::SlotRef(SlotId::new())),
        })),
        op: BinaryOp::Eq,
        rhs: Box::new(Expr::Literal(LiteralSpec::Activity(act_id))),
    };
}

//=========================================================//
//                        TYPE                             //
//                      (derived)                          //
//=========================================================//

// NOTES
// - Some operators depend on NOIR info from the catalog: if a select attribute is ordered, we can say
//   attr < "5.13a".
//   - That could justify using the same for hours(duration) and km(length)

// Maybe rename DataTy -> TyKind?
// Just a tag/discriminator.
enum DataTy {
    Boolean,
    Int,
    Float,
    String,
    Timestamp,
    Length,   // To support: km(len), and to disallow: len + duration.
    Duration, // To support: hours(entry.duration)
    Mass,
    StringSet,
    Entry,
    Activity, // Activity is a type: for one, we need to support making a Literal out of it, to say activity == Activity(id: abc123).
}

// Wrap the actual *Ty struct.
enum AnyTy {
    Boolean,
    Int(IntTy),
    Float,
    String,
    Timestamp,
    Length,
    Duration(DurationTy),
    Mass,
    StringSet,
    Entry,
    Activity,
}
struct IntTy {
    min: Option<i32>,
    max: Option<i32>,
}
impl Ty for IntTy {
    type Carrier = i32;

    fn kind(&self) -> DataTy {
        DataTy::Int
    }

    fn level_of_measurement(&self) -> LevelOfMeasurement {
        LevelOfMeasurement::Ratio // Int isn't closed under division, but it's still meaningful.
        // Division just returns a float.
    }

    fn domain(&self) -> Domain<Self::Carrier> {
        Domain::Range(self.min.clone(), self.max.clone()) // Or Domain::Open iff both are None.
    }

    fn order(&self) -> Option<Order<Self::Carrier>> {
        Some(Order::Natural)
    }
}

struct DurationTy {}
impl Ty for DurationTy {
    type Carrier = Duration;

    fn kind(&self) -> DataTy {
        DataTy::Duration
    }

    fn level_of_measurement(&self) -> LevelOfMeasurement {
        LevelOfMeasurement::Ratio
    }

    fn domain(&self) -> Domain<Self::Carrier> {
        Domain::Open
    }

    fn order(&self) -> Option<Order<Self::Carrier>> {
        Some(Order::Natural)
    }
}

// Not sure if a trait is the right way to go, but it helps sketch out the info we need.
// Note that all the return types are from the perspective of something expecting *any* type.
// But the connections between the methods are lost, eg if kind() returns DataTy::Int, we know
// that Order should be Natural.
trait Ty {
    type Carrier;
    fn kind(&self) -> DataTy;
    fn level_of_measurement(&self) -> LevelOfMeasurement;
    fn domain(&self) -> Domain<Self::Carrier>;
    fn order(&self) -> Option<Order<Self::Carrier>>;
}

enum Domain<C> {
    Discrete(Vec<C>), // SelectAttr: ["V0", "V1", ..., "V10"]
    // Activity:   ["PushUp", "BenchPress", "LandminePress"]
    Open, // Any member of the type.
    Range(Option<C>, Option<C>), // NumericAttr:       [0, inf)
          // NumericAttr (RPE): [0, 10]
}

enum Order<C> {
    Natural, // Numeric ordering for numerics, lexical for string, etc.
    Discrete(Vec<C>),
}

enum LevelOfMeasurement {
    Nominal,
    Ordinal,
    Interval,
    Ratio,
}

//=========================================================//
//                       VALUE                             //
//                    (execution)                          //
//=========================================================//

// A value is fully-resolved, it's an actual number/string, etc.
// ??? Spec and Type are caried *with* a value, not on it. ???

// Same variants as DataTy.
// Should the contained data have wrapped structs, like BoolValue, IntValue, etc.?
// I'm picturing this + something like
//    ResolvedValue { spec: ExprSpec, ty: Box<dyn Ty>, }
enum Cell {
    Boolean(bool),
    Int(i32),
    Float(f32),
    String(String),
    Timestamp(DateTime<Utc>),
    Length(LengthMeasurement),
    Duration(Duration),
    Mass(MassMeasurement),
    StringSet(Vec<String>),
    Entry(Uuid),
    Activity(Uuid),
}
