//! The GPU's H264 engines on Windows (windows-medios): none yet. Each one
//! fails to start at once, so [`super::super::encode`] and
//! [`super::super::decode`] go on with OpenH264 as they do when Linux has no
//! GPU engine. Media Foundation's hardware encoders would go here.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::super::decode::Unit;
use super::super::Frame;

pub(crate) use super::super::engine::after;
pub use super::super::engine::{Engine, HARDWARE};

/// A unit out of an encoder, and how long its picture took.
pub(crate) type Made = (Unit, Duration);

/// A GPU encoder, which never starts here.
pub(crate) struct Encoder {
    pub(crate) engine: Engine,
    pub(crate) size: (u32, u32),
    never: Infallible,
}

impl Encoder {
    pub(crate) fn start(_: Engine, _: (u32, u32), _: u32, _: u32) -> Option<Self> {
        None
    }

    pub(crate) fn set_bitrate(&self, _: u32) {
        match self.never {}
    }

    pub(crate) fn busy(&self) -> bool {
        match self.never {}
    }

    pub(crate) fn push(&mut self, _: &Arc<Frame>, _: bool, _: Instant) -> bool {
        match self.never {}
    }

    pub(crate) fn pull(&mut self) -> Result<Option<Made>, ()> {
        match self.never {}
    }
}

/// A GPU decoder, which never starts here.
pub(crate) struct Decoder {
    pub(crate) engine: Engine,
    never: Infallible,
}

impl Decoder {
    pub(crate) fn start(_: Engine) -> Option<Self> {
        None
    }

    pub(crate) fn behind(&self) -> bool {
        match self.never {}
    }

    pub(crate) fn push(&mut self, _: Vec<u8>) -> bool {
        match self.never {}
    }

    pub(crate) fn pull(&mut self) -> Result<Option<Frame>, ()> {
        match self.never {}
    }
}
