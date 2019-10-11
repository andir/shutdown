use futures::Future;

mod attributes;
mod home_assistant;
mod state;

pub use attributes::Attributes;
pub use home_assistant::{HomeAssistant, HomeAssistantConfiguration};
pub use state::State;

#[derive(Debug)]
pub enum Error {
    HomeAssistant(home_assistant::Error),
}

impl From<home_assistant::Error> for Error {
    fn from(e: home_assistant::Error) -> Error {
        Error::HomeAssistant(e)
    }
}

pub trait Hass {
    fn get_state(
        &self,
        name: impl AsRef<str>,
    ) -> Box<dyn Future<Item = State, Error = Error> + Send>;
    fn set_state(
        &self,
        name: impl AsRef<str>,
        attributes: Attributes,
    ) -> Box<dyn Future<Item = State, Error = Error> + Send>;
    fn call_service(
        &self,
        domain: impl AsRef<str>,
        name: impl AsRef<str>,
        attributes: Option<Attributes>,
    ) -> Box<dyn Future<Item = Vec<State>, Error = Error> + Send>;
}

pub fn set_temperature(
    hass: &impl Hass,
    entity: impl AsRef<str>,
    temperature: f32,
) -> impl Future<Item = Vec<State>, Error = Error> {
    hass.call_service(
        "climate",
        "set_temperature",
        Some(
            Attributes::new()
                .set("entity_id", entity.as_ref())
                .set("temperature", temperature),
        ),
    )
    .map(|r| {
        println!("response: {:?}", r);
        r
    })
}
