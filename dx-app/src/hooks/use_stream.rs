use dioxus::prelude::*;
use futures_util::{Stream, StreamExt};
use gv_core::error::Result;

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
        }
    });
    signal
}
