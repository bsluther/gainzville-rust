//! Template instantiation: deep-copy an activity's template subtree into a set
//! of fresh log entries and values.

use std::collections::HashMap;

use uuid::Uuid;

use crate::io::Io;
use crate::models::{
    attribute::Value,
    entry::{Entry, Position, Temporal},
};

/// Deep-copy a template subtree into fresh log entries + values.
///
/// `subtree` must contain the entry identified by `root_id` plus all of its
/// descendants (i.e. the output of `FindDescendants` on the template root).
/// Every entry is assigned a new id; `is_template`/`is_complete` are cleared.
///
/// - The **root** takes the caller's `root_position` and `root_temporal` (where
///   the user is placing the instance — a day root or inside a sequence).
/// - Every **non-root** entry keeps its template `position.frac_index` (to
///   preserve sibling order) and its template `temporal`, but its
///   `position.parent_id` is remapped to the *new* id of its parent. Children's
///   `position` is therefore `Some`, never `None` — only the root may be
///   placed at a forest root.
///
/// Values are re-keyed to the new entry ids; a value whose entry isn't in the
/// subtree is dropped.
///
/// `is_template` sets the kind of every instantiated entry: `false` materializes
/// the subtree into the log; `true` composes it into another template (the
/// caller is embedding the subtree under a template parent).
pub fn instantiate_subtree(
    io: &dyn Io,
    root_id: Uuid,
    subtree: &[Entry],
    values: &[Value],
    root_position: Option<Position>,
    root_temporal: Temporal,
    is_template: bool,
) -> (Vec<Entry>, Vec<Value>) {
    // Stable old-id -> new-id map for every entry in the subtree.
    let id_map: HashMap<Uuid, Uuid> = subtree.iter().map(|e| (e.id, io.uuid())).collect();

    let entries = subtree
        .iter()
        .map(|e| {
            let new_id = id_map[&e.id];
            if e.id == root_id {
                // Merge template duration into root temporal.
                let temporal = Temporal::parse(
                    root_temporal.start(),
                    root_temporal.end(),
                    e.temporal.duration(),
                )
                .unwrap_or(root_temporal.clone());
                Entry::from_template(e, new_id, root_position.clone(), temporal, is_template)
            } else {
                let position = e.position.as_ref().map(|p| Position {
                    // Parent is within the subtree (descendants are connected),
                    // so the map has it; fall back defensively to the old id.
                    parent_id: id_map.get(&p.parent_id).copied().unwrap_or(p.parent_id),
                    frac_index: p.frac_index.clone(),
                });
                Entry::from_template(e, new_id, position, e.temporal.clone(), is_template)
            }
        })
        .collect();

    let new_values = values
        .iter()
        .filter_map(|v| {
            id_map
                .get(&v.entry_id)
                .map(|&new_entry_id| Value::from_template(v, new_entry_id))
        })
        .collect();

    (entries, new_values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::attribute::{AttributeValue, NumericValue};
    use fractional_index::FractionalIndex;

    fn template_entry(id: Uuid, parent: Option<Uuid>, is_sequence: bool) -> Entry {
        Entry {
            id,
            activity_id: None,
            owner_id: Uuid::new_v4(),
            name: None,
            position: parent.map(|parent_id| Position {
                parent_id,
                frac_index: FractionalIndex::default(),
            }),
            is_template: true,
            display_as_sets: false,
            is_sequence,
            is_complete: false,
            temporal: Temporal::None,
        }
    }

    fn numeric_value(entry_id: Uuid, attr_id: Uuid, v: f64) -> Value {
        Value {
            entry_id,
            attribute_id: attr_id,
            index_float: None,
            index_string: None,
            plan: None,
            actual: Some(AttributeValue::Numeric(NumericValue::Exact(v))),
        }
    }

    #[test]
    fn root_gets_fresh_id_and_caller_position() {
        let root_id = Uuid::new_v4();
        let root = template_entry(root_id, None, true);
        let day_root = Uuid::new_v4();
        let pos = Position {
            parent_id: day_root,
            frac_index: FractionalIndex::default(),
        };

        let (entries, _) = instantiate_subtree(
            &crate::io::SystemIo::default(),
            root_id,
            &[root],
            &[],
            Some(pos.clone()),
            Temporal::Start {
                start: chrono::Utc::now(),
            },
            false,
        );

        assert_eq!(entries.len(), 1);
        let inst_root = &entries[0];
        assert_ne!(inst_root.id, root_id, "root id must be fresh");
        assert!(!inst_root.is_template);
        assert_eq!(inst_root.position.as_ref().unwrap().parent_id, day_root);
        assert!(matches!(inst_root.temporal, Temporal::Start { .. }));
    }

    #[test]
    fn children_parents_remap_through_levels() {
        // root -> child -> grandchild
        let root_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();
        let grandchild_id = Uuid::new_v4();
        let subtree = vec![
            template_entry(root_id, None, true),
            template_entry(child_id, Some(root_id), true),
            template_entry(grandchild_id, Some(child_id), false),
        ];

        let (entries, _) = instantiate_subtree(
            &crate::io::SystemIo::default(),
            root_id,
            &subtree,
            &[],
            None,
            Temporal::None,
            false,
        );

        // Map each instance back to which template entry it came from by structure.
        let inst_root = entries.iter().find(|e| e.position.is_none()).unwrap();
        let inst_child = entries
            .iter()
            .find(|e| e.position.as_ref().map(|p| p.parent_id) == Some(inst_root.id))
            .unwrap();
        let inst_grandchild = entries
            .iter()
            .find(|e| e.position.as_ref().map(|p| p.parent_id) == Some(inst_child.id))
            .unwrap();

        // All fresh ids, none matching the template, all non-template.
        for e in &entries {
            assert!(!e.is_template);
            assert!(!e.is_complete);
        }
        assert_ne!(inst_child.id, child_id);
        assert_ne!(inst_grandchild.id, grandchild_id);
        // Grandchild's parent is the *new* child id, not the template's.
        assert_eq!(
            inst_grandchild.position.as_ref().unwrap().parent_id,
            inst_child.id
        );
    }

    #[test]
    fn values_rekey_to_new_entry_ids() {
        let root_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();
        let attr = Uuid::new_v4();
        let subtree = vec![
            template_entry(root_id, None, true),
            template_entry(child_id, Some(root_id), false),
        ];
        let values = vec![numeric_value(child_id, attr, 7.0)];

        let (entries, new_values) = instantiate_subtree(
            &crate::io::SystemIo::default(),
            root_id,
            &subtree,
            &values,
            None,
            Temporal::None,
            false,
        );

        let inst_child = entries.iter().find(|e| e.position.is_some()).unwrap();
        assert_eq!(new_values.len(), 1);
        assert_eq!(
            new_values[0].entry_id, inst_child.id,
            "value re-keyed to new child id"
        );
        assert_eq!(new_values[0].attribute_id, attr);
    }

    #[test]
    fn is_template_true_composes_as_template() {
        // Embedding into another template: instantiated entries stay templates.
        let root_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();
        let subtree = vec![
            template_entry(root_id, None, true),
            template_entry(child_id, Some(root_id), false),
        ];
        let parent = Uuid::new_v4();
        let pos = Position {
            parent_id: parent,
            frac_index: FractionalIndex::default(),
        };

        let (entries, _) = instantiate_subtree(
            &crate::io::SystemIo::default(),
            root_id,
            &subtree,
            &[],
            Some(pos),
            Temporal::None,
            true,
        );

        assert_eq!(entries.len(), 2);
        assert!(
            entries.iter().all(|e| e.is_template),
            "composed subtree stays template"
        );
    }

    #[test]
    fn scalar_template_instantiates_as_single_entry() {
        let root_id = Uuid::new_v4();
        let attr = Uuid::new_v4();
        let subtree = vec![template_entry(root_id, None, false)];
        let values = vec![numeric_value(root_id, attr, 3.0)];

        let (entries, new_values) = instantiate_subtree(
            &crate::io::SystemIo::default(),
            root_id,
            &subtree,
            &values,
            Some(Position {
                parent_id: Uuid::new_v4(),
                frac_index: FractionalIndex::default(),
            }),
            Temporal::None,
            false,
        );

        assert_eq!(entries.len(), 1);
        assert!(!entries[0].is_sequence);
        assert_eq!(new_values.len(), 1);
        assert_eq!(new_values[0].entry_id, entries[0].id);
    }
}
