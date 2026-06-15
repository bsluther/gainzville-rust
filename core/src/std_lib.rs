use fractional_index::FractionalIndex;
use uuid::{Uuid, uuid};

use crate::{
    DEFAULT_USER_ID,
    models::{
        activity::{Activity, ActivityName},
        attribute::{
            Attribute, AttributeValue, LengthConfig, LengthUnit, MassConfig, MassMeasurement,
            MassUnit, MassValue, MultiselectConfig, NumericConfig, NumericValue, SelectConfig,
            TextConfig, Value,
        },
        entry::{Entry, Position, Temporal},
    },
};

// -----------------------------------------------------------------------------
// Stable ids
//
// Std-lib items are seeded idempotently: on every launch the client checks each
// id and creates the item only if it's missing (see `SqliteClient::seed_std_lib`).
// That requires the ids to be STABLE across runs, so they're hardcoded here
// rather than minted with `Uuid::new_v4()`.
//
// NOTE: the values below are placeholders — deliberately fake/sequential so
// they're easy to find and swap. Replace each with a freshly generated v4 UUID
// before distributing a build.
// -----------------------------------------------------------------------------

const REPS_ID: Uuid = uuid!("00000000-0000-4000-8000-000000000001");
const LOAD_ID: Uuid = uuid!("00000000-0000-4000-8000-000000000002");
const OUTCOME_ID: Uuid = uuid!("00000000-0000-4000-8000-000000000003");
const YDS_GRADE_ID: Uuid = uuid!("00000000-0000-4000-8000-000000000004");
const RPE_ID: Uuid = uuid!("00000000-0000-4000-8000-000000000005");
const V_GRADE_ID: Uuid = uuid!("00000000-0000-4000-8000-000000000006");
const GRIP_TYPE_ID: Uuid = uuid!("00000000-0000-4000-8000-000000000007");
const HOLD_SIZE_ID: Uuid = uuid!("00000000-0000-4000-8000-000000000008");
const DISTANCE_ID: Uuid = uuid!("00000000-0000-4000-8000-000000000009");
const NOTES_ID: Uuid = uuid!("00000000-0000-4000-8000-00000000000a");
const LOCATION_ID: Uuid = uuid!("00000000-0000-4000-8000-00000000000b");
const CLIMB_TAG_ID: Uuid = uuid!("00000000-0000-4000-8000-00000000000c");

/// Stable ids for one std-lib activity: its own id plus the ids of its template
/// entries. Grouping keeps each activity's ids together and lets call sites read
/// `PULL_UP.id` / `PULL_UP.template.id` instead of a flat wall of consts.
struct ActivityIds {
    id: Uuid,
    template: TemplateIds,
}

struct TemplateIds {
    /// Template root entry id. Any deeper template entries (sequence members,
    /// sets) derive their ids from this one — see `strength_workout`.
    id: Uuid,
}

impl ActivityIds {
    const fn new(id: Uuid, root: Uuid) -> Self {
        ActivityIds {
            id,
            template: TemplateIds { id: root },
        }
    }
}

const PULL_UP: ActivityIds = ActivityIds::new(
    uuid!("00000000-0000-4000-8000-000000000101"),
    uuid!("00000000-0000-4000-8000-000000000102"),
);

const STRENGTH_WORKOUT: ActivityIds = ActivityIds::new(
    uuid!("00000000-0000-4000-8000-000000000201"),
    uuid!("00000000-0000-4000-8000-000000000202"),
);

const BOULDER: ActivityIds = ActivityIds::new(
    uuid!("00000000-0000-4000-8000-000000000301"),
    uuid!("00000000-0000-4000-8000-000000000302"),
);

const SPORT_CLIMB: ActivityIds = ActivityIds::new(
    uuid!("00000000-0000-4000-8000-000000000311"),
    uuid!("00000000-0000-4000-8000-000000000312"),
);

const BENCH_PRESS: ActivityIds = ActivityIds::new(
    uuid!("00000000-0000-4000-8000-000000000321"),
    uuid!("00000000-0000-4000-8000-000000000322"),
);

const SINGLE_LEG_RDL: ActivityIds = ActivityIds::new(
    uuid!("00000000-0000-4000-8000-000000000331"),
    uuid!("00000000-0000-4000-8000-000000000332"),
);

const RUN: ActivityIds = ActivityIds::new(
    uuid!("00000000-0000-4000-8000-000000000341"),
    uuid!("00000000-0000-4000-8000-000000000342"),
);

const REPEATERS: ActivityIds = ActivityIds::new(
    uuid!("00000000-0000-4000-8000-000000000351"),
    uuid!("00000000-0000-4000-8000-000000000352"),
);

const DUMBBELL_OVERHEAD_PRESS: ActivityIds = ActivityIds::new(
    uuid!("00000000-0000-4000-8000-000000000361"),
    uuid!("00000000-0000-4000-8000-000000000362"),
);

const DUMBBELL_LATERAL_RAISE: ActivityIds = ActivityIds::new(
    uuid!("00000000-0000-4000-8000-000000000371"),
    uuid!("00000000-0000-4000-8000-000000000372"),
);

const ECCENTRIC_WRIST_CURL: ActivityIds = ActivityIds::new(
    uuid!("00000000-0000-4000-8000-000000000381"),
    uuid!("00000000-0000-4000-8000-000000000382"),
);

const REAR_ELEVATED_SPLIT_SQUAT: ActivityIds = ActivityIds::new(
    uuid!("00000000-0000-4000-8000-000000000391"),
    uuid!("00000000-0000-4000-8000-000000000392"),
);

/// A std-lib activity bundled with its template subtree and any values to attach
/// to template entries. The client seeds these as a `CreateActivity` followed by
/// a `CreateValue` per `template_values` entry.
pub struct StdLibActivity {
    pub activity: Activity,
    pub template: Vec<Entry>,
    pub template_values: Vec<Value>,
}

pub struct StandardLibrary {}

impl StandardLibrary {
    pub fn attributes() -> Vec<Attribute> {
        let reps = Attribute {
            id: REPS_ID,
            owner_id: DEFAULT_USER_ID,
            name: "Reps".to_string(),
            description: Some("Number of repetitions performed".to_string()),
            config: NumericConfig {
                min: Some(0.),
                max: None,
                integer: true,
                default: Some(0.),
            }
            .into(),
        };

        let load = Attribute {
            id: LOAD_ID,
            owner_id: DEFAULT_USER_ID,
            name: "Load".to_string(),
            description: Some("External resistance or weight".to_string()),
            config: MassConfig {
                default_unit: MassUnit::Pound,
            }
            .into(),
        };

        let outcome = Attribute {
            id: OUTCOME_ID,
            owner_id: DEFAULT_USER_ID,
            name: "Outcome".to_string(),
            description: Some(
                "How a climbing attempt ended (sent, flash, onsight, attempt, working)".to_string(),
            ),
            config: SelectConfig {
                options: vec![
                    "Sent".to_string(),
                    "Flash".to_string(),
                    "Onsight".to_string(),
                    "Attempt".to_string(),
                    "Working".to_string(),
                ],
                default: None,
                ordered: false,
            }
            .into(),
        };

        // This is a good example where you want something like an equivalence, so you can use 10-
        // but map that to 10a or 10b if someone else uses that, or map to French grades.
        let yds_grade = Attribute {
            id: YDS_GRADE_ID,
            owner_id: DEFAULT_USER_ID,
            name: "YDS Grade".to_string(),
            description: Some("Yosemite Decimal System climbing grade".to_string()),
            config: SelectConfig {
                options: vec![
                    "5.8".to_string(),
                    "5.9".to_string(),
                    "10-".to_string(),
                    "10".to_string(),
                    "10+".to_string(),
                    "11-".to_string(),
                    "11".to_string(),
                    "11+".to_string(),
                    "12-".to_string(),
                    "12".to_string(),
                    "12+".to_string(),
                    "13-".to_string(),
                    "13".to_string(),
                    "13+".to_string(),
                ],
                default: None,
                ordered: true,
            }
            .into(),
        };

        let rpe = Attribute {
            id: RPE_ID,
            owner_id: DEFAULT_USER_ID,
            name: "RPE".to_string(),
            description: Some(
                "Rate of perceived exertion. How difficult something felt on a scale from 1 (minimal) to 10 (maximal)".to_string()),
            config: NumericConfig {
                min: Some(1.),
                max: Some(10.),
                integer: false,
                default: None,
            }.into()
        };

        let v_grade = Attribute {
            id: V_GRADE_ID,
            owner_id: DEFAULT_USER_ID,
            name: "V Grade".to_string(),
            description: Some("Hueco V-scale bouldering grade".to_string()),
            config: SelectConfig {
                options: (0..=12).map(|n| format!("V{n}")).collect(),
                default: None,
                ordered: true,
            }
            .into(),
        };

        let grip_type = Attribute {
            id: GRIP_TYPE_ID,
            owner_id: DEFAULT_USER_ID,
            name: "Grip Type".to_string(),
            description: Some(
                "Hand position used when training fingers for rock climbing.".to_string(),
            ),
            config: SelectConfig {
                options: vec![
                    "Open hand".to_string(),
                    "Half crimp".to_string(),
                    "Full crimp".to_string(),
                    "Front 2".to_string(),
                    "Front 3".to_string(),
                ],
                default: None,
                ordered: false,
            }
            .into(),
        };

        let hold_size = Attribute {
            id: HOLD_SIZE_ID,
            owner_id: DEFAULT_USER_ID,
            name: "Hold Size".to_string(),
            description: Some(
                "The size of a climbing hold used during finger training, in millimeters."
                    .to_string(),
            ),
            config: NumericConfig {
                min: Some(1.0),
                max: None,
                integer: true,
                default: None,
            }
            .into(),
        };

        let distance = Attribute {
            id: DISTANCE_ID,
            owner_id: DEFAULT_USER_ID,
            name: "Distance".to_string(),
            description: Some(
                "How far — the distance of a run, swim, ride, or climb approach.".to_string(),
            ),
            config: LengthConfig {
                default_unit: LengthUnit::Mile,
            }
            .into(),
        };

        let notes = Attribute {
            id: NOTES_ID,
            owner_id: DEFAULT_USER_ID,
            name: "Notes".to_string(),
            description: Some("Free-form notes about an entry.".to_string()),
            config: TextConfig {
                default: None,
                autocomplete: false,
            }
            .into(),
        };

        let location = Attribute {
            id: LOCATION_ID,
            owner_id: DEFAULT_USER_ID,
            name: "Location".to_string(),
            description: Some("Where it happened — a gym, crag, or trailhead.".to_string()),
            config: TextConfig {
                default: None,
                autocomplete: true,
            }
            .into(),
        };

        let climb_tag = Attribute {
            id: CLIMB_TAG_ID,
            owner_id: DEFAULT_USER_ID,
            name: "Climb Tag".to_string(),
            description: Some(
                "Free-form tags describing a climb — how it was done, the wall, or the style."
                    .to_string(),
            ),
            config: MultiselectConfig {
                options: vec![
                    "warm up".to_string(),
                    "repeat".to_string(),
                    "working".to_string(),
                    "with downclimb".to_string(),
                    "traverse".to_string(),
                    "moon board".to_string(),
                    "tension board".to_string(),
                    "lead".to_string(),
                    "top-rope".to_string(),
                    "trad".to_string(),
                    "crack".to_string(),
                ],
                default: None,
            }
            .into(),
        };

        vec![
            reps, load, outcome, yds_grade, rpe, v_grade, grip_type, hold_size, distance, notes,
            location, climb_tag,
        ]
    }

    pub fn activities() -> Vec<StdLibActivity> {
        vec![
            scalar_activity(
                &PULL_UP,
                "Pull Up",
                "Standard pull-up, bodyweight or with additional load.",
                &[REPS_ID],
            ),
            scalar_activity(
                &BOULDER,
                "Boulder",
                "A rock or gym climb performed without a rope, usually 10-20 feet tall.",
                &[V_GRADE_ID, OUTCOME_ID, CLIMB_TAG_ID],
            ),
            scalar_activity(
                &SPORT_CLIMB,
                "Sport Climb",
                "A rock or gym climb equipped with bolts, usually 30-200 feet tall.",
                &[YDS_GRADE_ID, OUTCOME_ID, CLIMB_TAG_ID],
            ),
            scalar_activity(
                &BENCH_PRESS,
                "Bench Press",
                "A horizontal press of a barbell or dumbbells while lying on a bench.",
                &[REPS_ID, LOAD_ID],
            ),
            scalar_activity(
                &SINGLE_LEG_RDL,
                "Single-Leg RDL",
                "Single-leg Romanian deadlift: a unilateral hip-hinge training the hamstrings, glutes, and balance.",
                &[REPS_ID, LOAD_ID],
            ),
            scalar_activity(
                &RUN,
                "Run",
                "A run on road, trail, track, or treadmill.",
                &[RPE_ID],
            ),
            scalar_activity(
                &REPEATERS,
                "Repeaters",
                "Fingerboard training protocol usually targeting power-endurance. Alternate reps (~5-10 seconds) with short rest (~3 seconds).",
                &[GRIP_TYPE_ID, RPE_ID, LOAD_ID, HOLD_SIZE_ID],
            ),
            scalar_activity(
                &DUMBBELL_OVERHEAD_PRESS,
                "Dumbbell Overhead Press",
                "A standing or seated vertical press of dumbbells from the shoulders to overhead.",
                &[LOAD_ID, REPS_ID, RPE_ID],
            ),
            scalar_activity(
                &DUMBBELL_LATERAL_RAISE,
                "Dumbbell Lateral Raise",
                "Raising dumbbells out to the sides to shoulder height, targeting the lateral deltoids.",
                &[LOAD_ID, REPS_ID, RPE_ID],
            ),
            scalar_activity(
                &ECCENTRIC_WRIST_CURL,
                "Eccentric Wrist Curl",
                "Eccentric-only wrist flexor exercise used to treat golfer's elbow (medial epicondylitis). Emphasis is on slow eccentric motion over a 20-60 second set, rather than number of reps. Avoiding concentric effort during resets seems to improve results.",
                &[LOAD_ID],
            ),
            scalar_activity(
                &REAR_ELEVATED_SPLIT_SQUAT,
                "Rear-Elevated Split Squat",
                "Split squat with the rear foot elevated on a surface like a bench or box. Good for building stability and supporting muscles used in running and climbing. Common loading mechanisms are handheld dumbbells, barbell, or a kettlebell in goblet grip.",
                &[REPS_ID, LOAD_ID],
            ),
            // Seeded last: its set members reference the activities above, which
            // must already be committed (each activity is its own transaction).
            Self::strength_workout(),
        ]
    }

    /// A full-body session: a sequence of exercises, each a `display_as_sets`
    /// sequence of 5 sets. Set values are prescriptions, so they go on `actual`
    /// (the Swift app currently reads `actual`, not `plan`). The "sets 1-2 then
    /// sets 3-5" shape recurs, so each exercise is built from two set profiles.
    fn strength_workout() -> StdLibActivity {
        let activity = Activity {
            id: STRENGTH_WORKOUT.id,
            owner_id: DEFAULT_USER_ID,
            source_activity_id: None,
            name: ActivityName::parse("Strength Workout".to_string()).expect("valid std-lib name"),
            description: Some(
                "General strength workout targeting a mix of pull, push, and single-leg."
                    .to_string(),
            ),
        };

        let root = template_entry(
            STRENGTH_WORKOUT.template.id,
            Some(STRENGTH_WORKOUT.id),
            None,
            true, // a sequence of exercises
        );

        // (exercise activity, name, per-set value lists). Members of each
        // exercise share the exercise's activity_id (the sets-shape rule). The
        // sequence node is named "<exercise> Sets" so it's labeled even without
        // the sets UI to derive a name from its members.
        let exercises = [
            (BENCH_PRESS.id, "Bench Press", bench_press_sets()),
            (SINGLE_LEG_RDL.id, "Single-Leg RDL", reps_rpe_sets()),
            (PULL_UP.id, "Pull Up", pull_up_sets()),
            (DUMBBELL_LATERAL_RAISE.id, "Dumbbell Lateral Raise", lateral_raise_sets()),
            (REAR_ELEVATED_SPLIT_SQUAT.id, "Rear-Elevated Split Squat", reps_rpe_sets()),
        ];

        let mut template = vec![root];
        let mut template_values = Vec::new();
        let exercise_positions = ordered_indices(exercises.len());

        for (exercise_index, ((member_activity, label, sets), frac)) in
            exercises.into_iter().zip(exercise_positions).enumerate()
        {
            let exercise_n = (exercise_index + 1) as u128;
            let seq_id = workout_entry_id(exercise_n * 0x10);
            template.push(sets_sequence_entry(
                seq_id,
                format!("{label} Sets"),
                Position {
                    parent_id: STRENGTH_WORKOUT.template.id,
                    frac_index: frac,
                },
            ));

            let set_positions = ordered_indices(sets.len());
            for (set_index, (set_values, set_frac)) in
                sets.into_iter().zip(set_positions).enumerate()
            {
                let set_id = workout_entry_id(exercise_n * 0x10 + (set_index + 1) as u128);
                template.push(template_entry(
                    set_id,
                    Some(member_activity),
                    Some(Position {
                        parent_id: seq_id,
                        frac_index: set_frac,
                    }),
                    false,
                ));
                for (attribute_id, value) in set_values {
                    template_values.push(actual_value(set_id, attribute_id, value));
                }
            }
        }

        StdLibActivity {
            activity,
            template,
            template_values,
        }
    }
}

// A single set's prescribed values: (attribute id, value to store on `actual`).
type SetValues = Vec<(Uuid, AttributeValue)>;

/// Two sets of profile `a` (sets 1-2) then three of profile `b` (sets 3-5) —
/// the loading shape every Strength Workout exercise follows.
fn two_then_three(a: SetValues, b: SetValues) -> Vec<SetValues> {
    vec![a.clone(), a, b.clone(), b.clone(), b]
}

fn bench_press_sets() -> Vec<SetValues> {
    two_then_three(
        vec![
            (REPS_ID, num_exact(8.0)),
            (RPE_ID, num_range(3.0, 5.0)),
            (LOAD_ID, lb_range(95.0, 115.0)),
        ],
        vec![
            (REPS_ID, num_exact(5.0)),
            (RPE_ID, num_range(8.0, 9.0)),
            (LOAD_ID, lb_range(135.0, 185.0)),
        ],
    )
}

fn pull_up_sets() -> Vec<SetValues> {
    two_then_three(
        vec![
            (REPS_ID, num_exact(8.0)),
            (RPE_ID, num_range(3.0, 6.0)),
            (LOAD_ID, lb_exact(0.0)),
        ],
        vec![
            (REPS_ID, num_range(2.0, 3.0)),
            (RPE_ID, num_range(8.0, 9.0)),
            (LOAD_ID, lb_range(70.0, 100.0)),
        ],
    )
}

fn lateral_raise_sets() -> Vec<SetValues> {
    two_then_three(
        vec![(REPS_ID, num_exact(8.0)), (LOAD_ID, lb_range(5.0, 10.0))],
        vec![(REPS_ID, num_exact(5.0)), (LOAD_ID, lb_range(20.0, 25.0))],
    )
}

/// Reps + RPE only, shared by Single-Leg RDL and Rear-Elevated Split Squat.
fn reps_rpe_sets() -> Vec<SetValues> {
    two_then_three(
        vec![(REPS_ID, num_exact(8.0)), (RPE_ID, num_range(3.0, 6.0))],
        vec![(REPS_ID, num_exact(5.0)), (RPE_ID, num_range(7.0, 8.0))],
    )
}

fn num_exact(v: f64) -> AttributeValue {
    AttributeValue::Numeric(NumericValue::Exact(v))
}

fn num_range(min: f64, max: f64) -> AttributeValue {
    AttributeValue::Numeric(NumericValue::Range { min, max })
}

fn lb_exact(v: f64) -> AttributeValue {
    AttributeValue::Mass(MassValue::Exact(MassMeasurement {
        unit: MassUnit::Pound,
        value: v,
    }))
}

fn lb_range(min: f64, max: f64) -> AttributeValue {
    AttributeValue::Mass(MassValue::Range {
        unit: MassUnit::Pound,
        min,
        max,
    })
}

/// Stable id for an entry inside the Strength Workout template. Internal entries
/// (exercise sequences, sets) live in the workout's reserved `0x02xx` id block,
/// after the activity (`0x0201`) and template root (`0x0202`): exercise `i` is
/// `0x0i0`, its set `j` is `0x0ij`.
fn workout_entry_id(low: u128) -> Uuid {
    Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0200 | low)
}

/// `n` fractional indices in ascending sibling order.
fn ordered_indices(n: usize) -> Vec<FractionalIndex> {
    let mut indices = Vec::with_capacity(n);
    let mut current = FractionalIndex::default();
    for _ in 0..n {
        indices.push(current.clone());
        current = FractionalIndex::new_after(&current);
    }
    indices
}

/// A named `display_as_sets` sequence node; its members (the sets) carry the
/// shared activity_id. The sets UI derives a label from those members, but the
/// explicit name keeps it readable before that UI exists.
fn sets_sequence_entry(id: Uuid, name: String, position: Position) -> Entry {
    Entry {
        id,
        activity_id: None,
        owner_id: DEFAULT_USER_ID,
        name: Some(name),
        position: Some(position),
        is_template: true,
        display_as_sets: true,
        is_sequence: true,
        is_complete: false,
        temporal: Temporal::None,
    }
}

/// A value carrying only an `actual` (no plan), for one attribute on one entry.
fn actual_value(entry_id: Uuid, attribute_id: Uuid, actual: AttributeValue) -> Value {
    Value {
        entry_id,
        attribute_id,
        index_float: None,
        index_string: None,
        plan: None,
        actual: Some(actual),
    }
}

/// Build a scalar (non-sequence) std-lib activity: a single template root with
/// each listed attribute attached as an empty value (no plan, no actual).
fn scalar_activity(
    ids: &ActivityIds,
    name: &str,
    description: &str,
    attribute_ids: &[Uuid],
) -> StdLibActivity {
    let activity = Activity {
        id: ids.id,
        owner_id: DEFAULT_USER_ID,
        source_activity_id: None,
        name: ActivityName::parse(name.to_string()).expect("valid std-lib name"),
        description: Some(description.to_string()),
    };
    let root = template_entry(ids.template.id, Some(ids.id), None, false);
    let template_values = attribute_ids
        .iter()
        .map(|&attribute_id| empty_value(ids.template.id, attribute_id))
        .collect();
    StdLibActivity {
        activity,
        template: vec![root],
        template_values,
    }
}

/// An attribute attached to an entry with neither a plan nor an actual value.
fn empty_value(entry_id: Uuid, attribute_id: Uuid) -> Value {
    Value {
        entry_id,
        attribute_id,
        index_float: None,
        index_string: None,
        plan: None,
        actual: None,
    }
}

/// Build a template entry owned by the default user with no temporal extent.
/// Templates carry duration-only temporals at most; std-lib seeds use none.
fn template_entry(
    id: Uuid,
    activity_id: Option<Uuid>,
    position: Option<Position>,
    is_sequence: bool,
) -> Entry {
    Entry {
        id,
        activity_id,
        owner_id: DEFAULT_USER_ID,
        name: None,
        position,
        is_template: true,
        display_as_sets: false,
        is_sequence,
        is_complete: false,
        temporal: Temporal::None,
    }
}
