//! `GET /api/events`: Server-Sent Events. `event: changed` says what in the
//! data directory changed ([`crate::watch`]), `event: job` carries a job
//! whose status or output moved on, and a `: ping` comment comes every 15 s.

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
    let stream = futures_util::stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let sse = match &event {
                        Event::Changed(c) => SseEvent::default().event("changed").json_data(c),
                        Event::Job(job) => SseEvent::default().event("job").json_data(job),
                    };
                    match sse {
                        Ok(sse) => return Some((Ok(sse), rx)),
                        Err(_) => continue,
                    }
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
