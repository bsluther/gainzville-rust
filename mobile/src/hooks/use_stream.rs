use dioxus::prelude::*;
use futures_util::{Stream, StreamExt};
use gv_core::{
    error::Result,
    models::{activity::Activity, entry::Entry, entry_view::EntryView},
    reader::Reader,
};
use gv_sqlite::{client::SqliteClient, reader::SqliteReader};
use std::sync::atomic::{AtomicUsize, Ordering};
use uuid::Uuid;

static STREAM_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Create a signal that reads from a stream. The stream must return a result, which will be mapped
/// into an option. The stream returns None before the first item is pulled from the stream or if
/// there is an error.
pub fn use_stream<T, S, F>(stream_fn: F) -> Signal<Option<T>>
where
    T: 'static + Clone,
    S: 'static + Stream<Item = Result<T>>,
    F: 'static + Fn() -> S,
{
    let mut signal = use_signal(|| None);

    use_resource(move || {
        let id = STREAM_COUNT.fetch_add(1, Ordering::SeqCst);
        debug!("Stream {} STARTED", id);
        let stream = stream_fn();
        async move {
            tokio::pin!(stream);

            while let Some(result) = stream.next().await {
                match result {
                    Ok(result) => {
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                        signal.set(Some(result));
                    }
                    Err(e) => {
                        eprintln!("Error reading stream in use_stream: {e}");
                        signal.set(None);
                    }
                }
            }
            debug!("Stream {} ENDED", id);
        }
    });
    signal
}

pub fn use_stream2<T, S, F>(stream_fn: F) -> Signal<Option<T>>
where
    T: 'static + Clone,
    S: 'static + Stream<Item = Result<T>>,
    F: 'static + FnOnce() -> S,
{
    use_hook(|| {
        let mut signal = Signal::new(None);
        let stream = stream_fn();

        // Dioxus's spawn - handles !Send types
        spawn(async move {
            tokio::pin!(stream);
            while let Some(result) = stream.next().await {
                match result {
                    Ok(result) => signal.set(Some(result)),
                    Err(e) => {
                        eprintln!("Error: {e}");
                        signal.set(None);
                    }
                }
            }
        });

        signal
    })
}

pub fn use_test_stream() -> Signal<i32> {
    use_hook(|| {
        let mut signal = Signal::new(0);

        spawn(async move {
            let mut count = 0;
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                count += 1;
                debug!("Timer updating signal to {}", count);
                signal.set(count);
            }
        });

        signal
    })
}

pub fn use_test_counter() -> Signal<i32> {
    let mut signal = use_signal(|| 0);

    use_future(move || async move {
        let mut count = 0;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            count += 1;
            debug!("Counter: {}", count);
            signal.set(count);
        }
    });

    signal
}

/// Custom hook that streams activities from the database
pub fn use_stream_all_activities() -> Signal<Vec<Activity>> {
    let mut signal = use_signal(Vec::new);
    let client = use_context::<SqliteClient>();

    use_resource(move || {
        let client = client.clone();
        async move {
            let stream = client.stream_activities();
            tokio::pin!(stream);

            while let Some(result) = stream.next().await {
                match result {
                    Ok(activities) => signal.set(activities),
                    Err(e) => eprintln!("Error fetching activities: {e}"),
                }
            }
        }
    });

    signal
}

pub fn use_stream_all_entries() -> ReadSignal<Vec<Entry>> {
    let mut signal = use_signal(Vec::new);
    let client = use_context::<SqliteClient>();

    // DIAGNOSTIC: Replace broadcast stream with polling
    // This tests if the issue is with broadcast::Receiver or with signal.set() in general
    use_future(move || {
        let client = client.clone();
        async move {
            loop {
                debug!("use_stream_all_entries: polling");
                match SqliteReader::all_entries(&client.pool).await {
                    Ok(entries) => {
                        debug!(
                            "use_stream_all_entries: got {} entries, setting signal",
                            entries.len()
                        );
                        signal.set(entries);
                        debug!("use_stream_all_entries: signal.set() complete");
                    }
                    Err(e) => {
                        debug!("Error fetching entries: {e}");
                    }
                }
                // Poll every 500ms instead of using broadcast stream
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    });

    signal.into()
}

pub fn use_stream_entry_view(id: ReadSignal<Uuid>) -> ReadSignal<Option<EntryView>> {
    let mut signal = use_signal(|| None);
    let client = use_context::<SqliteClient>();
    let current_id = *id.peek();

    // DIAGNOSTIC: Disable streaming, just do a one-time fetch
    // This tests if the freeze is caused by concurrent streams
    use_future(move || {
        let client = client.clone();
        async move {
            debug!(
                "use_stream_entry_view: one-time fetch for id={}",
                current_id
            );
            match SqliteReader::find_entry_view_by_id(&client.pool, current_id).await {
                Ok(Some(entry)) => {
                    debug!(
                        "use_stream_entry_view: got entry, setting signal id={}",
                        current_id
                    );
                    signal.set(Some(entry));
                }
                Ok(None) => {
                    debug!("use_stream_entry_view: entry not found id={}", current_id);
                    signal.set(None);
                }
                Err(e) => {
                    debug!("use_stream_entry_view: error id={}: {}", current_id, e);
                    signal.set(None);
                }
            }
            debug!(
                "use_stream_entry_view: one-time fetch complete id={}",
                current_id
            );
        }
    });

    signal.into()
}
