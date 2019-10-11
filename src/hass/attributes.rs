use serde_json::Value;
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug)]
pub struct Attributes(HashMap<String, Value>);

impl Attributes {
    pub fn new() -> Self {
        Attributes(HashMap::new())
    }

    pub fn set(mut self, key: impl AsRef<str>, value: impl Into<Value>) -> Self {
        self.0.insert(key.as_ref().to_string(), value.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_attribute() {
        Attributes::new().set("foo", 1).set("baz", "foo");
    }
}
