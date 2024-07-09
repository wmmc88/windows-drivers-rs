use serde::ser::{self};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

/// This type represents all possible errors that can occur when serializing
/// or deserializing [`WDKMetadata`].
#[derive(Debug, Error)]
pub enum Error {
    /// catch-all error emitted during serialization, when a more specific
    /// error type is not available. This type of error is commonly
    /// generated from [`serde`]'s `derive` feature's generated `Serialize`
    /// impls.
    #[error("custom serialization error: {message}")]
    CustomSerialization { message: String },

    /// error emitted when an empty key name is encountered during
    /// serialization. Serialization of values always requires a non-empty
    /// key name
    #[error("empty key name encountered during serialization of value: {value_being_serialized}")]
    EmptySerializationKeyName { value_being_serialized: String },

    /// error emitted when duplicate key names are found during
    /// serialization. Serializing into a [`crate::ser::Map`] requires
    /// unique key names
    #[error(
        "duplicate keys found during serialization:\nkey: {key}\nvalue 1: {value_1}\nvalue 2: \
         {value_2}"
    )]
    DuplicateSerializationKeys {
        key: String,
        value_1: String,
        value_2: String,
    },
}

impl ser::Error for Error {
    fn custom<T: std::fmt::Display>(msg: T) -> Self {
        Error::CustomSerialization {
            message: msg.to_string(),
        }
    }
}