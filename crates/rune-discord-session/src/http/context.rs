//! One bounded around read. Its result never asserts a history boundary.

use rune_i18n::es;
use std::collections::HashSet;

use super::*;

impl Http {
    pub(super) async fn context(
        &self,
        secret: &SessionSecret,
        request: u64,
        channel: ChannelId,
        anchor: MessageId,
        limit: usize,
    ) -> Result<Vec<Event>, RestError> {
        if channel.0 == 0 || anchor.0 == 0 {
            return Err(RestError::response(wire::invalid(), false));
        }
        let limit = limit.clamp(1, 100);
        let path = format!("/channels/{channel}/messages?around={anchor}&limit={limit}");
        let data = self
            .request(secret, Method::GET, &path, None, 8 * 1024 * 1024)
            .await?;
        let rows = data
            .as_array()
            .filter(|rows| rows.len() <= limit)
            .ok_or_else(|| RestError::response(wire::invalid(), false))?;
        let mut seen = HashSet::with_capacity(rows.len());
        let mut messages = Vec::with_capacity(rows.len());
        let mut events = Vec::new();
        for row in rows {
            let (message, users) =
                wire::message(row).map_err(|reason| RestError::response(reason, false))?;
            if message.channel != channel || !seen.insert(message.id) {
                return Err(RestError::response(wire::invalid(), false));
            }
            messages.push(message);
            events.extend(users.into_iter().map(Event::UserChanged));
        }
        if !seen.contains(&anchor) {
            return Err(RestError::rejected(es(
                "No encontramos ese mensaje en este contexto.",
            )));
        }
        messages.sort_by_key(|message| message.id);
        events.push(Event::ContextLoaded {
            request,
            channel,
            anchor,
            messages,
        });
        Ok(events)
    }
}
