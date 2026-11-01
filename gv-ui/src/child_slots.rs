//! Drop slots between a sequence's children.
//!
//! A sequence renders as `drop, child, drop, child, ..., drop`: each drop slot is a gap a dragged
//! entry can land in, carrying the fractional position that lands it exactly there. Rust lays the
//! list out once per data version, so the Swift view never computes fractional indices or invents
//! identities. Whether a slot accepts *the entry being dragged* depends on drag state and stays a
//! read-once check at drag time (`WouldCreateCycle`, plus the no-op neighbour check below).

use fractional_index::FractionalIndex;
use gv_core::{
    forest::Forest,
    models::entry::{Entry, Position},
};
use uuid::Uuid;

/// One item in a sequence's rendered child list.
///
/// Entry slots carry `Entry`, not `EntryJoin`: the forest that answers this query holds entries
/// only, so carrying joins would make a pure in-memory answer depend on SQL, and would re-deliver
/// the whole list whenever any child's attributes change. Each child card subscribes to its own
/// `EntryJoin` instead.
#[derive(Debug, Clone, PartialEq)]
pub enum ChildSlot {
    /// A gap between two adjacent siblings (or before the first / after the last).
    Drop {
        /// Stable identity, derived from the neighbours: `drop:<pred|start>:<succ|end>`. It
        /// changes exactly when the gap's neighbours change, so UI state attached to a gap (e.g.
        /// "is targeted") never transfers to a different gap after a move.
        id: String,
        /// The position that lands an entry in this gap: same parent, fractional index strictly
        /// between the neighbours'.
        position: Position,
        /// Neighbour ids. Needed by the UI to reject no-op drops (dropping an entry next to
        /// itself), which depend on what is being dragged.
        pred_id: Option<Uuid>,
        succ_id: Option<Uuid>,
    },
    /// A child entry. Its identity is the entry id.
    Entry { entry: Entry },
}

impl ChildSlot {
    pub fn id(&self) -> String {
        match self {
            ChildSlot::Drop { id, .. } => id.clone(),
            ChildSlot::Entry { entry } => entry.id.to_string(),
        }
    }
}

fn drop_slot_id(pred: Option<Uuid>, succ: Option<Uuid>) -> String {
    let pred = pred.map_or_else(|| "start".to_string(), |id| id.to_string());
    let succ = succ.map_or_else(|| "end".to_string(), |id| id.to_string());
    format!("drop:{pred}:{succ}")
}

/// The slot list for `parent_id`'s children: alternating drop and entry slots, starting and ending
/// with a drop slot, children in fractional-index order.
///
/// Total over live data: a missing or non-sequence parent yields no slots (the card is about to
/// change shape). If two siblings share a fractional index (corrupt data; no position fits
/// strictly between them) the gap between them gets no drop slot, so alternation holds only while
/// sibling indices are strictly increasing — which the domain maintains.
pub fn child_slots(forest: &Forest, parent_id: Uuid) -> Vec<ChildSlot> {
    if !forest.entry(parent_id).is_some_and(|p| p.is_sequence) {
        return Vec::new();
    }
    let children = forest.children(parent_id);
    let mut slots = Vec::with_capacity(children.len() * 2 + 1);
    for i in 0..=children.len() {
        let pred = i.checked_sub(1).map(|j| children[j]);
        let succ = children.get(i).copied();
        let frac_index = match (
            pred.and_then(Entry::frac_index),
            succ.and_then(Entry::frac_index),
        ) {
            (None, None) => Some(FractionalIndex::default()),
            (Some(p), None) => Some(FractionalIndex::new_after(p)),
            (None, Some(s)) => Some(FractionalIndex::new_before(s)),
            (Some(p), Some(s)) => FractionalIndex::new_between(p, s),
        };
        if let Some(frac_index) = frac_index {
            let (pred_id, succ_id) = (pred.map(|e| e.id), succ.map(|e| e.id));
            slots.push(ChildSlot::Drop {
                id: drop_slot_id(pred_id, succ_id),
                position: Position {
                    parent_id,
                    frac_index,
                },
                pred_id,
                succ_id,
            });
        }
        if let Some(child) = succ {
            slots.push(ChildSlot::Entry {
                entry: child.clone(),
            });
        }
    }
    slots
}

/// Checks the slot-list invariants against the forest it was computed from, Matrix
/// `DateDividerAdjuster`-style. Returns a description of the first violation.
pub fn check_child_slots(
    forest: &Forest,
    parent_id: Uuid,
    slots: &[ChildSlot],
) -> Result<(), String> {
    let children = forest.children(parent_id);
    if slots.len() != children.len() * 2 + 1 {
        return Err(format!(
            "expected {} slots for {} children, got {}",
            children.len() * 2 + 1,
            children.len(),
            slots.len()
        ));
    }
    let mut ids = std::collections::HashSet::new();
    for (i, slot) in slots.iter().enumerate() {
        if !ids.insert(slot.id()) {
            return Err(format!("duplicate slot id {}", slot.id()));
        }
        match (i % 2, slot) {
            (
                0,
                ChildSlot::Drop {
                    id,
                    position,
                    pred_id,
                    succ_id,
                },
            ) => {
                let k = i / 2;
                let pred = k.checked_sub(1).map(|j| children[j]);
                let succ = children.get(k).copied();
                if *pred_id != pred.map(|e| e.id) || *succ_id != succ.map(|e| e.id) {
                    return Err(format!("slot {i}: neighbours don't match the children"));
                }
                if *id != drop_slot_id(*pred_id, *succ_id) {
                    return Err(format!("slot {i}: id not derived from neighbours"));
                }
                if position.parent_id != parent_id {
                    return Err(format!("slot {i}: position has the wrong parent"));
                }
                if pred
                    .and_then(Entry::frac_index)
                    .is_some_and(|p| *p >= position.frac_index)
                {
                    return Err(format!("slot {i}: position not after predecessor"));
                }
                if succ
                    .and_then(Entry::frac_index)
                    .is_some_and(|s| *s <= position.frac_index)
                {
                    return Err(format!("slot {i}: position not before successor"));
                }
            }
            (1, ChildSlot::Entry { entry }) => {
                if entry.id != children[i / 2].id {
                    return Err(format!("slot {i}: entry out of order"));
                }
            }
            _ => return Err(format!("slot {i}: drop/entry slots don't alternate")),
        }
    }
    Ok(())
}

/// The members a sets card shows: `sequence_id`'s children in order, or nothing if it doesn't
/// display as sets. The same children, in the same order, as its `child_slots` entry slots.
pub fn sets_members(forest: &Forest, sequence_id: Uuid) -> Vec<Entry> {
    if !forest.entry(sequence_id).is_some_and(|e| e.display_as_sets) {
        return Vec::new();
    }
    forest.children(sequence_id).into_iter().cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gv_core::models::entry::Temporal;
    use proptest::prelude::*;

    fn entry(position: Option<Position>, is_sequence: bool) -> Entry {
        Entry {
            id: Uuid::new_v4(),
            activity_id: None,
            owner_id: Uuid::nil(),
            name: None,
            position,
            is_template: false,
            display_as_sets: false,
            is_sequence,
            is_complete: false,
            temporal: Temporal::None,
        }
    }

    fn child_of(parent: Uuid, frac_index: FractionalIndex) -> Entry {
        entry(
            Some(Position {
                parent_id: parent,
                frac_index,
            }),
            false,
        )
    }

    #[test]
    fn sets_members_are_the_entry_slots_of_a_sets_parent() {
        let mut parent = entry(None, true);
        parent.display_as_sets = true;
        let first = FractionalIndex::default();
        let second = FractionalIndex::new_after(&first);
        let (a, b) = (child_of(parent.id, first), child_of(parent.id, second));
        let forest = Forest::from(vec![parent.clone(), b.clone(), a.clone()]);

        let members: Vec<Uuid> = sets_members(&forest, parent.id)
            .iter()
            .map(|e| e.id)
            .collect();
        let entry_slots: Vec<Uuid> = child_slots(&forest, parent.id)
            .iter()
            .filter_map(|slot| match slot {
                ChildSlot::Entry { entry } => Some(entry.id),
                ChildSlot::Drop { .. } => None,
            })
            .collect();
        assert_eq!(members, vec![a.id, b.id]);
        assert_eq!(members, entry_slots);
    }

    #[test]
    fn sets_members_are_empty_unless_displayed_as_sets() {
        let parent = entry(None, true);
        let child = child_of(parent.id, FractionalIndex::default());
        let forest = Forest::from(vec![parent.clone(), child]);
        assert!(sets_members(&forest, parent.id).is_empty());
        assert!(sets_members(&forest, Uuid::new_v4()).is_empty());
    }

    /// Strictly increasing fractional indices built by random insertions, the way the app
    /// creates them (append, prepend, insert between).
    fn indices_from_gaps(gaps: &[usize]) -> Vec<FractionalIndex> {
        let mut out: Vec<FractionalIndex> = Vec::new();
        for &g in gaps {
            let at = g % (out.len() + 1);
            let fi = match (at.checked_sub(1).map(|j| &out[j]), out.get(at)) {
                (None, None) => FractionalIndex::default(),
                (Some(p), None) => FractionalIndex::new_after(p),
                (None, Some(s)) => FractionalIndex::new_before(s),
                (Some(p), Some(s)) => FractionalIndex::new_between(p, s).unwrap(),
            };
            out.insert(at, fi);
        }
        out
    }

    #[test]
    fn empty_sequence_has_one_drop_slot() {
        let parent = entry(None, true);
        let forest = Forest::from(vec![parent.clone()]);
        let slots = child_slots(&forest, parent.id);
        assert_eq!(slots.len(), 1);
        assert!(
            matches!(&slots[0], ChildSlot::Drop { id, pred_id: None, succ_id: None, .. } if id == "drop:start:end")
        );
        check_child_slots(&forest, parent.id, &slots).unwrap();
    }

    #[test]
    fn missing_or_scalar_parent_has_no_slots() {
        let scalar = entry(None, false);
        let forest = Forest::from(vec![scalar.clone()]);
        assert!(child_slots(&forest, scalar.id).is_empty());
        assert!(child_slots(&forest, Uuid::new_v4()).is_empty());
    }

    #[test]
    fn duplicate_indices_skip_the_gap_instead_of_panicking() {
        let parent = entry(None, true);
        let fi = FractionalIndex::default();
        let a = child_of(parent.id, fi.clone());
        let b = child_of(parent.id, fi);
        let forest = Forest::from(vec![parent.clone(), a, b]);
        let slots = child_slots(&forest, parent.id);
        // drop, a, (no drop), b, drop
        assert_eq!(slots.len(), 4);
    }

    fn forest_strategy() -> impl Strategy<Value = (Forest, Uuid)> {
        (
            prop::collection::vec(0usize..64, 0..12),
            prop::collection::vec(0usize..64, 0..6),
        )
            .prop_flat_map(|(gaps, noise_gaps)| {
                let parent = entry(None, true);
                let other = entry(None, true);
                let mut all = vec![parent.clone(), other.clone()];
                all.extend(
                    indices_from_gaps(&gaps)
                        .into_iter()
                        .map(|fi| child_of(parent.id, fi)),
                );
                // Siblings under another parent must not leak in.
                all.extend(
                    indices_from_gaps(&noise_gaps)
                        .into_iter()
                        .map(|fi| child_of(other.id, fi)),
                );
                (Just(all).prop_shuffle(), Just(parent.id))
            })
            .prop_map(|(all, parent_id)| (Forest::from(all), parent_id))
    }

    proptest! {
        /// Slots alternate drop/entry, start and end with a drop, list the children in order,
        /// every position sits strictly between its neighbours, and ids are unique and derived
        /// from the neighbours.
        #[test]
        fn slots_satisfy_invariants((forest, parent_id) in forest_strategy()) {
            let slots = child_slots(&forest, parent_id);
            prop_assert_eq!(check_child_slots(&forest, parent_id, &slots), Ok(()));
        }

        /// Dropping an entry at slot k lands it exactly in that gap, and only that gap's id
        /// changes: every other slot keeps its identity, and two new gaps appear around the
        /// dropped entry.
        #[test]
        fn dropping_into_a_slot_lands_there_and_keeps_other_ids(
            (forest, parent_id) in forest_strategy(),
            pick in 0usize..64,
        ) {
            let before = child_slots(&forest, parent_id);
            let drops: Vec<_> = before.iter().filter_map(|s| match s {
                ChildSlot::Drop { id, position, pred_id, succ_id } =>
                    Some((id.clone(), position.clone(), *pred_id, *succ_id)),
                _ => None,
            }).collect();
            let (gap_id, position, pred_id, succ_id) = drops[pick % drops.len()].clone();

            let moved = entry(Some(position), false);
            let mut all: Vec<Entry> = vec![moved.clone()];
            all.extend(forest_entries(&forest));
            let after_forest = Forest::from(all);
            let after = child_slots(&after_forest, parent_id);
            prop_assert_eq!(check_child_slots(&after_forest, parent_id, &after), Ok(()));

            // Lands between the slot's neighbours.
            let order: Vec<Uuid> = after_forest.children(parent_id).iter().map(|e| e.id).collect();
            let at = order.iter().position(|id| *id == moved.id).unwrap();
            prop_assert_eq!(at.checked_sub(1).map(|j| order[j]), pred_id);
            prop_assert_eq!(order.get(at + 1).copied(), succ_id);

            // Identity stability.
            let ids = |slots: &[ChildSlot]| slots.iter().map(ChildSlot::id).collect::<std::collections::HashSet<_>>();
            let (b, a) = (ids(&before), ids(&after));
            let removed: Vec<_> = b.difference(&a).cloned().collect();
            prop_assert_eq!(removed, vec![gap_id]);
            prop_assert_eq!(a.difference(&b).count(), 3); // two new gaps + the moved entry
        }
    }

    fn forest_entries(forest: &Forest) -> Vec<Entry> {
        // Forest exposes no iterator; rebuild from roots + descendants.
        forest
            .roots()
            .into_iter()
            .flat_map(|r| forest.descendants(r.id))
            .cloned()
            .collect()
    }
}
