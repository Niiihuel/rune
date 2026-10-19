//! A person's answer to an hCaptcha that stopped one request, sent back once
//! with that same request. Never solved here: the host's window shows the
//! challenge and the person solves it.

use std::sync::{Arc, Mutex};

use reqwest::{header::HeaderValue, RequestBuilder};
use zeroize::Zeroizing;

use super::Http;
use crate::ChallengeAnswer;

/// A person's answer to the challenge that stopped one request, with the
/// values that bind it to that same request.
pub(crate) struct Solved {
    pub answer: ChallengeAnswer,
    pub rqtoken: Option<Zeroizing<String>>,
    pub session: Option<Zeroizing<String>>,
}

/// The answer for the write `route` names (`"POST /channels/1/messages"`).
pub(crate) struct Answer {
    pub route: String,
    pub solved: Solved,
}

impl Http {
    /// This transport, carrying `answer` for the next request on its route.
    pub(crate) fn answering(&self, answer: Answer) -> Self {
        Self {
            answer: Arc::new(Mutex::new(Some(answer))),
            ..self.clone()
        }
    }

    /// The answer for a request on `route`; taken, so it goes out once.
    pub(super) fn answer_for(&self, route: &str) -> Option<Solved> {
        let mut answer = self.answer.lock().ok()?;
        if answer.as_ref()?.route != route {
            return None;
        }
        answer.take().map(|answer| answer.solved)
    }
}

/// `request` with the headers the official client adds when it repeats a
/// request after its challenge (see docs/REA.md); Serein sends the same.
pub(super) fn with_answer(
    mut request: RequestBuilder,
    solved: &Solved,
) -> Result<RequestBuilder, ()> {
    for (name, value) in [
        ("X-Captcha-Key", Some(&solved.answer.0)),
        ("X-Captcha-Rqtoken", solved.rqtoken.as_ref()),
        ("X-Captcha-Session-Id", solved.session.as_ref()),
    ] {
        let Some(value) = value else { continue };
        let mut value = HeaderValue::from_str(value).map_err(|_| ())?;
        value.set_sensitive(true);
        request = request.header(name, value);
    }
    Ok(request)
}
