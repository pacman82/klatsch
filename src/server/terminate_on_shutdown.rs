use std::pin::pin;

use async_stream::stream;
use futures_util::Stream;
use tokio::{select, sync::watch};
use tokio_stream::StreamExt;

/// Wrap a stream to terminate when a shutting down is `true`. We use this in our http layer to end
/// requests during shutdown.
pub fn terminate_on_shutdown<I>(
    org: impl Stream<Item = I>,
    mut shutting_down: watch::Receiver<bool>,
) -> impl Stream<Item = I> {
    stream! {
        let mut org = pin!(org);
        loop {
            select! {
                biased;
                result = shutting_down.changed() => {
                    result.expect(
                        "Shutting down watch channel must not be closed as long as receivers exist."
                    );
                    if *shutting_down.borrow() {
                        break;
                    }
                }
                maybe_item = org.next() => {
                    if let Some(item) = maybe_item {
                        yield item;
                    } else {
                        break;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use futures_util::stream;
    use tokio::time::timeout;
    use tokio_stream::StreamExt;

    #[tokio::test]
    async fn shutdown_terminates_stream_with_infinite_items() {
        // Given a terminatable stream with otherwise infini
        let (tx, rx) = watch::channel(false);
        let org_stream = stream::repeat(42);
        let mut term_stream = pin!(terminate_on_shutdown(org_stream, rx));

        // When `true` is sent on the shutdown channel
        tx.send(true).unwrap();

        // Then the stream yields no more items
        let item = term_stream.next().await;
        assert!(item.is_none());
    }

    #[tokio::test]
    async fn terminating_stream_is_immediate() {
        // Given a terminatable stream which would yield its next item in five seconds
        let (tx, rx) = watch::channel(false);
        let org_stream = stream::repeat(42).throttle(Duration::from_secs(5)).skip(1);
        let mut term_stream = pin!(terminate_on_shutdown(org_stream, rx));

        // When `true` is sent on the shutdown channel
        tx.send(true).unwrap();

        // Then the stream returns `None` right away
        let item = term_stream.next().await;
        assert!(item.is_none());
    }

    #[tokio::test]
    async fn terminate_if_underlying_stream_ends_even_if_shutting_down_is_false() {
        // Given a stream with two items
        let (_tx, rx) = watch::channel(false);
        let org_stream = stream::repeat(42).take(2);
        let term_stream = terminate_on_shutdown(org_stream, rx);

        // When collecting items
        let items = timeout(Duration::from_secs(1), term_stream.collect::<Vec<_>>()).await;

        // Then the stream yields both items and terminates, even though shutting_down is false
        let items = items.expect("Stream did not terminate in time");
        assert_eq!(2, items.len())
    }
}
