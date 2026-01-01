use std::sync::LazyLock;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::{
    error::FileError,
    storage::provider::{CACHE_PATH, StorageProvider},
};

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Context {
    current_scope: String,
}

impl Context {
    pub fn scope(&self) -> &str {
        &self.current_scope
    }

    pub fn into_scope(self) -> String {
        self.current_scope
    }

    pub fn change_scope(mut self, scope: Option<String>) -> Self {
        if let Some(scope) = scope {
            self.current_scope = scope;
        } else {
            self.current_scope.clear();
        }

        self
    }
}

impl StorageProvider for Context {
    #[inline]
    fn work_file() -> &'static LazyLock<Box<str>> {
        &CACHE_PATH
    }

    fn serialize(&self) -> Result<String> {
        Ok(serde_json::to_string(self).map_err(FileError::Json)?)
    }

    fn deserialize(data: &str) -> Result<Self> {
        Ok(serde_json::from_str(data).map_err(FileError::Json)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::provider::StorageProvider;

    #[test]
    fn scope_and_into_scope() {
        let ctx = Context {
            current_scope: "test".into(),
        };
        assert_eq!(ctx.scope(), "test");
        assert_eq!(ctx.into_scope(), "test");
    }

    #[test]
    fn change_scope() {
        let ctx = Context::default();
        assert!(ctx.scope().is_empty());
        let ctx = ctx.change_scope(Some("test".into()));
        assert_eq!(ctx.scope(), "test");
    }

    #[test]
    fn serialize_deserialize() {
        let ctx = Context {
            current_scope: "test".into(),
        };
        if let Ok(serialized) = StorageProvider::serialize(&ctx) {
            assert_eq!(serialized, r#"{"current_scope":"test"}"#);
            let deserialized: Result<Context> = StorageProvider::deserialize(&serialized);
            assert!(deserialized.is_ok_and(|c| c.scope() == "test"));
        } else {
            panic!("Context serialization failed");
        }
    }

    #[test]
    fn deserialize_invalid_json() {
        let data = "{ invalid json }";
        let result: Result<Context> = StorageProvider::deserialize(data);
        assert!(result.is_err());
    }
}
