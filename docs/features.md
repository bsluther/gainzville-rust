This is not an exhaustive list; the other docs include many features which fall under the model,
collaboration, or other categories.

### Standard Library
Many activities and attributes are common and well-defined and can be included as part of
Gainzville's standard library available to all users. Ideally this would be curated and maintained
by professionals. A standard library helps avoid duplication of common exercises. All standard
library activities and attributes would be defind in the same model as the user-defined entities.

### Visual Descriptions

### Analysis and Visualization
Allow user to ask questions like "how many attempts to make on boulder problems that are v8 or above
each week" or "what is my total training load in the weeks preceding an injury"?

Visualize progress (time, load, distance, speed, any other attrbute) in custom plots.

### Coaching

### Data Import / Sync
Data sources I'd want to import
- Markdown. I have three vaults: 
    - Personal laptop with daily log of studying, working on GV, etc.
    - Phone with daily log of training, sleeping, etc.
    - Work laptop with daily log of hours.
- Strava. The only place I store most of my cardio workouts.

### Data Export
- Possible formats: JSON, parquet, sqlite file, human-readable text, csv.
- Possible denormalization schemes:
    - Fully denormalized, as is.
    - Resolve attributes onto entries but not the forest strucutre (similar to EntryJoin).
    - Fully denormalized. I don't see this being that useful as a data access format. But it might
    be for a human readable format.
```
    2026-02-13
    Woke up
        5:00am
    Stone Age
        2pm - 4:10pm
        Autobelays
            YDS: 5.10-
            Outcome: repeat
        Autobelays
            YDS: 5.11-
            Outcome: repeat
        Autobelays
            YDS: 5.12-
            Outcome: repeat
            Notes: Challenging, but fun! 
        Boulder Problem
            V-Grade: V3
            Outcome: flash
        Boulder Problem
            V-Grade: V2
            Outcome: flash
```
- Replace ID's with semantic strings where possible. E.g. "bsluther/yds-grade" or some such.
That way you can actually read the data
```json
{
    "entries": [
        {
            "id": "77bdf8f4-0cf5-4a57-9a0a-af1d3e057119", // Don't think you can make this semantic.
            "activity_id": "bsluther/pull-ups",
            "attributes": {
                "bsluther/yds-grade": {
                    "plan": {
                        // ...
                    },
                    "actual": {
                        // ...
                    }
                }
            }
            // ...
        }
    ]
}
```

- Denormalized.