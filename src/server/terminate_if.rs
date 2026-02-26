use std::pin::pin;

use async_stream::stream;
use futures_util::Stream;
use tokio::{select, sync::watch};
use tokio_stream::StreamExt;

/// Wrap a stream to terminate when the watch signal becomes `true`. If the sender is dropped the
/// remaining items are forwarded.
pub fn terminate_if<I>(
    org: impl Stream<Item = I>,
    mut signal: watch::Receiver<bool>,
) -> impl Stream<Item = I> {
    stream! {
        let mut org = pin!(org);
        loop {
            select! {
                biased;
                result = signal.changed() => {
                    match result {
                        // Signal true; Terminate stream.
                        Ok(()) if *signal.borrow_and_update() => break,
                        // Signal false; Do nothing.
                        Ok(()) => {}
                        // Sender dropped; Assume signal never becomes `true`. Forward remaining
                        // items.
                        Err(_) => {
                            while let Some(item) = org.next().await {
                                yield item;
                            }
                            break;
                        }
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
    async fn signal_terminates_stream_with_infinite_items() {
        // Given a terminatable stream with otherwise infinite items
        let (tx, rx) = watch::channel(false);
        let org_stream = stream::repeat(42);
        let mut term_stream = pin!(terminate_if(org_stream, rx));

        // When `true` is sent on the signal channel
        tx.send(true).unwrap();

        // Then the stream yields no more items
        let item = term_stream.next().await;
        assert!(item.is_none());
    }

    #[tokio::test]
    async fn termination_is_immediate() {
        // Given a terminatable stream which would yield its next item in five seconds
        let (tx, rx) = watch::channel(false);
        let org_stream = stream::repeat(42).throttle(Duration::from_secs(5)).skip(1);
        let mut term_stream = pin!(terminate_if(org_stream, rx));

        // When `true` is sent on the signal channel
        tx.send(true).unwrap();

        // Then the stream returns `None` right away
        let item = term_stream.next().await;
        assert!(item.is_none());
    }

    #[tokio::test]
    async fn terminates_when_underlying_stream_ends() {
        // Given a stream with two items
        let (_, rx) = watch::channel(false);
        let org_stream = stream::repeat(42).take(2);
        let term_stream = terminate_if(org_stream, rx);

        // When collecting items
        let items = timeout(Duration::from_secs(1), term_stream.collect::<Vec<_>>()).await;

        // Then the stream yields both items and terminates
        let items = items.expect("Stream did not terminate in time");
        assert_eq!(2, items.len())
    }

    #[tokio::test]
    async fn dropped_sender_forwards_remaining_items() {
        // Given a stream with two items and no sender
        let (tx, rx) = watch::channel(false);
        drop(tx);
        let org_stream = stream::repeat(42).take(2);
        let term_stream = terminate_if(org_stream, rx);

        // When collecting items
        let items = timeout(Duration::from_secs(1), term_stream.collect::<Vec<_>>()).await;

        // Then the stream still yields both items
        let items = items.expect("Stream did not terminate in time");
        assert_eq!(2, items.len())
    }
}
