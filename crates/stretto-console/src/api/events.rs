//! `GET /api/events`: Server-Sent Events. `event: changed` says what in the
//! data directory changed ([`crate::watch`]), `event: job` carries a job
//! whose status or output moved on, and a `: ping` comment comes every 15 s.
//! The stream ends when the console stops ([`crate::State::stop`]).

use crate::watch::Event;
use crate::Shared;
use axum::extract::State;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use futures_util::Stream;
use std::convert::Infallible;
use std::time::Duration;
use tokio::sync::broadcast::error::RecvError;

pub async fn events(
    State(state): State<Shared>,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let rx = state.events.subscribe();
    let stopping = state.stopping.subscribe();
    let stream =
        futures_util::stream::unfold((rx, stopping), |(mut rx, mut stopping)| async move {
            loop {
                let received = tokio::select! {
                    biased;
                    received = rx.recv() => received,
                    _ = stopping.wait_for(|stop| *stop) => return None,
                };
                match received {
                    Ok(event) => {
                        let (name, data) = match &event {
                            Event::Changed(c) => ("changed", serde_json::to_string(c)),
                            Event::Job(job) => ("job", serde_json::to_string(job)),
                        };
                        let sse = SseEvent::default().event(name);
                        return Some((Ok(sse.data(data.unwrap_or_default())), (rx, stopping)));
                    }
                    // A slow client misses events; the next one it gets says
                    // what changed since.
                    Err(RecvError::Lagged(_)) => continue,
                    Err(RecvError::Closed) => return None,
                }
            }
        });
    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    )
}
