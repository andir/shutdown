use super::Attributes;

#[derive(Serialize, Deserialize, Debug)]
pub struct State {
    // name of the state
    #[serde(rename = "state")]
    pub name: String,
    pub attributes: Option<Attributes>,
}

impl State {
    pub fn new(name: impl AsRef<str>, attributes: Option<Attributes>) -> Self {
        State {
            name: name.as_ref().to_string(),
            attributes: attributes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_state_with_attributes() {
        let attrs = Attributes::new()
            .set("foo", "bar")
            .set("baz", 1.0)
            .set("zes", false);

        State::new("some_state", Some(attrs));
    }
}
