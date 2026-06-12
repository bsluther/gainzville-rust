/// What a debounce/blur commit would dispatch for a value field: an update,
/// or a clear (`UpdateAttributeValue` with `nil` — empties the field while
/// keeping the attribute attached). Shared by the numeric and mass editors,
/// whose commit paths produce one of these or nothing.
enum PendingWrite<Value> {
    case set(Value)
    case clear
}
