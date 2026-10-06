//! `GET /api/events`: a Server-Sent Events stream of the signed-in account's
//! events (chat streaming, TTS audio, scene progress, ...). Plain HTTP, so it
//! works with the browser's `EventSource` and the session cookie, no extra
//! protocol. See `events` for how events are routed to accounts.

use std::convert::Infallible;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::Stream;
use tokio::sync::broadcast::error::RecvError;

use super::util::ActiveUser;
use super::WebState;

pub async fn stream(
    State(st): State<WebState>,
    ActiveUser(authed): ActiveUser,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = st.bus.subscribe();
    let me = authed.user.id.clone();
    let stream = futures::stream::unfold(rx, move |mut rx| {
        let me = me.clone();
        async move {
            loop {
                match rx.recv().await {
                    Ok(ev) if ev.visible_to(&me) => {
                        let sse = Event::default()
                            .event(ev.name.clone())
                            .json_data(&ev.payload)
                            .unwrap_or_else(|_| Event::default().event(ev.name.clone()));
                        return Some((Ok(sse), rx));
                    }
                    Ok(_) => continue,
                    // A slow client missed some events; keep going with newer ones.
                    Err(RecvError::Lagged(skipped)) => {
                        tracing::warn!("[web] event stream lagged, dropped {skipped} events");
                        continue;
                    }
                    Err(RecvError::Closed) => return None,
                }
            }
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}
