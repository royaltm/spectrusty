/*
    Copyright (C) 2020  Rafal Michalski

    This file is part of SPECTRUSTY, a Rust library for building emulators.

    For the full copyright notice, see the lib.rs file.
*/
//! Platform dependend audio device streaming implementations.
//!
//! To make use of this module enable one of the available features to the `spectrusty_audio` entry in `[dependencies]`
//! section of the Cargo configuration file.
use core::fmt;
use std::error::Error;

#[cfg(feature = "cpal")]
pub mod cpal;

#[cfg(feature = "sdl2")]
pub mod sdl2;

/// A list specifying categories of [AudioHandleError] error.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AudioHandleErrorKind {
    /// An error occuring when audio subsystem host or device is not available.
    AudioSubsystem,
    /// An error occuring while trying to create or modify an audio stream.
    AudioStream,
    /// An error occuring due to the invalid specification of the desired audio parameters or other arguments.
    InvalidArguments,
}

/// A common error type returned by all audio handle implementation methods.
#[derive(Debug, Clone)]
pub struct AudioHandleError {
    description: String,
    kind: AudioHandleErrorKind
}

impl fmt::Display for AudioHandleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.description.fmt(f)
    }
}

impl Error for AudioHandleError {}

impl AudioHandleError {
    /// Returns the corresponding category for this error.
    pub fn kind(&self) -> AudioHandleErrorKind {
        self.kind
    }
}

impl From<(String, AudioHandleErrorKind)> for AudioHandleError {
    fn from((description, kind): (String, AudioHandleErrorKind)) -> Self {
        AudioHandleError { description, kind }
    }
}
