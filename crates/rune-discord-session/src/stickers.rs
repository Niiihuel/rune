//! Sticker metadata only; the public image worker never receives credentials.

use rune_model::{RemoteImage, Sticker, StickerId};
use serde_json::Value;

use crate::{wire, LoginError};

pub(crate) fn decode(value: &Value, catalog: bool) -> Result<Vec<Sticker>, LoginError> {
    if value.is_null() {
        return Ok(Vec::new());
    }
    let items = value
        .as_array()
        .filter(|items| items.len() <= if catalog { 500 } else { 3 })
        .ok_or_else(wire::invalid)?;
    let mut seen = std::collections::HashSet::new();
    items
        .iter()
        .map(|item| {
            let id = StickerId(wire::id(&item["id"])?);
            if !seen.insert(id) {
                return Err(wire::invalid());
            }
            let name = item["name"]
                .as_str()
                .filter(|name| {
                    !name.trim().is_empty()
                        && name.len() <= 256
                        && !name.chars().any(char::is_control)
                })
                .ok_or_else(wire::invalid)?;
            // The documented GIF sticker origin differs from PNG/APNG; an
            // animated vector (Lottie) one is its JSON, as Serein reads it.
            let url = match item["format_type"].as_u64() {
                Some(1 | 2) => Some(format!("https://cdn.discordapp.com/stickers/{id}.png")),
                Some(3) => Some(format!("https://cdn.discordapp.com/stickers/{id}.json")),
                Some(4) => Some(format!("https://media.discordapp.net/stickers/{id}.gif")),
                _ => None,
            };
            let preview = url.is_some();
            Ok(Sticker {
                id,
                name: name.into(),
                image: url.map(|url| RemoteImage {
                    url,
                    kind: Default::default(),
                }),
                available: catalog
                    && preview
                    && item
                        .get("available")
                        .is_none_or(|value| value.as_bool() == Some(true)),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn raster_stickers_use_the_documented_origin_and_only_available_items_can_be_selected() {
        let values = json!([
            {"id":"1","name":"Hello","format_type":1},
            {"id":"2","name":"Wave","format_type":2,"available":false},
            {"id":"3","name":"Vector","format_type":3},
            {"id":"4","name":"Gif","format_type":4}
        ]);
        let catalog = decode(&values, true).unwrap();
        assert!(catalog[0].available);
        assert_eq!(
            catalog[0].image.as_ref().unwrap().url,
            "https://cdn.discordapp.com/stickers/1.png"
        );
        assert!(catalog[1].image.is_some() && !catalog[1].available);
        assert!(catalog[2].available);
        assert_eq!(
            catalog[2].image.as_ref().unwrap().url,
            "https://cdn.discordapp.com/stickers/3.json"
        );
        assert!(catalog[3].available);
        assert_eq!(
            catalog[3].image.as_ref().unwrap().url,
            "https://media.discordapp.net/stickers/4.gif"
        );
        assert!(
            decode(&values, false).is_err(),
            "a message is limited to three stickers"
        );
    }

    #[test]
    fn received_gif_stickers_keep_their_preview_without_claiming_send_availability() {
        let values = json!([
            {"id":"4","name":"Gif","format_type":4},
            {"id":"5","name":"Unavailable GIF","format_type":4,"available":false},
            {"id":"6","name":"Future format","format_type":99}
        ]);
        let received = decode(&values, false).unwrap();
        assert!(received.iter().all(|item| !item.available));
        assert!(received[0].image.is_some() && received[1].image.is_some());
        let catalog = decode(&values, true).unwrap();
        assert!(catalog[0].available);
        assert!(!catalog[1].available && catalog[1].image.is_some());
        assert!(!catalog[2].available && catalog[2].image.is_none());
    }

    #[test]
    fn duplicate_and_malformed_stickers_are_rejected() {
        for data in [
            json!([{"id":"1","name":"One","format_type":1},{"id":"1","name":"Two","format_type":1}]),
            json!([{"id":"../path","name":"One","format_type":1}]),
            json!([{"id":"1","name":"bad\nname","format_type":1}]),
        ] {
            assert!(decode(&data, true).is_err());
        }
    }
}
