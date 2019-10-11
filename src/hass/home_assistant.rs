use super::{Attributes, Error as HassError, Hass, State};
use futures::Future;
use reqwest::{
    r#async::{Client, ClientBuilder},
    Error as ReqwestError, IntoUrl, Url, UrlError,
};

#[derive(Debug)]
pub enum Error {
    UrlCanNotBeABase,
    UrlParse(UrlError),
    Reqwest(ReqwestError),
}

impl Into<Error> for UrlError {
    fn into(self) -> Error {
        Error::UrlParse(self)
    }
}

impl From<ReqwestError> for Error {
    fn from(e: ReqwestError) -> Error {
        Error::Reqwest(e)
    }
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Default)]
pub struct HomeAssistantConfiguration {
    verify_certs: bool,
}

impl HomeAssistantConfiguration {
    pub fn new() -> Self {
        HomeAssistantConfiguration {
            verify_certs: true,
            ..HomeAssistantConfiguration::default()
        }
    }

    pub fn set_verify_certs(mut self, value: bool) -> Self {
        self.verify_certs = value;
        self
    }
}

pub struct HomeAssistant {
    base_url: Url,
    client: Client,
}

impl HomeAssistant {
    pub fn new(base_url: impl IntoUrl, conf: Option<HomeAssistantConfiguration>) -> Result<Self> {
        let conf = conf.unwrap_or_else(|| HomeAssistantConfiguration::new());
        let client = ClientBuilder::new()
            .danger_accept_invalid_certs(!conf.verify_certs)
            .build()?;

        let base_url = base_url.into_url()?;

        if base_url.cannot_be_a_base() {
            return Err(Error::UrlCanNotBeABase);
        }

        Ok(Self { base_url, client })
    }

    fn new_state_url(&self, name: impl AsRef<str>) -> Url {
        let mut url = self.base_url.clone();
        let mut segments = url.path_segments_mut().unwrap(); // can be base has been checked in `fn new()`
        segments.push("api").push("states").push(name.as_ref());
        drop(segments); //  drop mutable borrow
        url
    }

    fn new_service_url(&self, domain: impl AsRef<str>, name: impl AsRef<str>) -> Url {
        let mut url = self.base_url.clone();
        let mut segments = url.path_segments_mut().unwrap(); // can be base has been checked in `fn new()`
        segments
            .push("api")
            .push("services")
            .push(domain.as_ref())
            .push(name.as_ref());
        drop(segments); //  drop mutable borrow
        url
    }
}

impl Hass for HomeAssistant {
    fn get_state(
        &self,
        name: impl AsRef<str>,
    ) -> Box<dyn Future<Item = State, Error = HassError> + Send> {
        let url = self.new_state_url(name);
        Box::new(
            self.client
                .get(url)
                .send()
                .and_then(|r| r.error_for_status())
                .and_then(|mut r| r.json())
                .map_err(|e| Error::from(e).into()),
        )
    }

    fn set_state(
        &self,
        name: impl AsRef<str>,
        attributes: Attributes,
    ) -> Box<dyn Future<Item = State, Error = HassError> + Send> {
        let url = self.new_state_url(&name);
        let new_state = State::new(name, Some(attributes));
        Box::new(
            self.client
                .post(url)
                .json(&new_state)
                .send()
                .and_then(|r| r.error_for_status())
                .and_then(|mut r| r.json())
                .map_err(|e| Error::from(e).into()),
        )
    }

    fn call_service(
        &self,
        domain: impl AsRef<str>,
        name: impl AsRef<str>,
        attributes: Option<Attributes>,
    ) -> Box<dyn Future<Item = Vec<State>, Error = HassError> + Send> {
        let url = self.new_service_url(domain, name);
        let req = self.client.post(url);
        let req = match attributes {
            None => req,
            Some(x) => req.json(&x),
        };
        Box::new(
            req.send()
                .and_then(|r| {
                    println!("error for status");
                    r.error_for_status()
                })
                .and_then(|mut r| {
                    let j = r.json();
                    println!("requesting json");
                    j
                })
                .map_err(|e| {
                    println!("calling service failed: {:?}", e);
                    Error::from(e).into()
                }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::IntoFuture;

    fn run_one<F>(f: F) -> std::result::Result<F::Item, F::Error>
    where
        F: IntoFuture,
        F::Future: Send + 'static,
        F::Item: Send + 'static,
        F::Error: Send + 'static,
    {
        let mut runtime = tokio::runtime::Runtime::new().expect("Unable to create a runtime");
        runtime.block_on(f.into_future())
    }

    #[test]
    fn construct_config() {
        HomeAssistantConfiguration::new().set_verify_certs(true);
    }

    #[test]
    fn construct_home_assistant() {
        HomeAssistant::new("https://foo", None).expect("failed to parse host foo?");
        HomeAssistant::new("https://foo:1234", None)
            .expect("failed to parse host foo with port 1234");
        HomeAssistant::new("https://foo:1234/some/prefix", None)
            .expect("failed to parse host foo with port 1234 and prefix");
    }

    #[test]
    fn get_state() {
        let hass = HomeAssistant::new(
            "https://hub.w17.io",
            Some(HomeAssistantConfiguration::new().set_verify_certs(false)),
        )
        .expect("failed to construct w17 client?!?");
        let result = run_one(hass.get_state("climate.lounge_wandthermostat"));

        assert!(result.is_ok());
        println!("state: {:?}", result.unwrap());
    }
    #[test]
    fn set_state() {
        let hass = HomeAssistant::new(
            "https://hub.w17.io",
            Some(HomeAssistantConfiguration::new().set_verify_certs(false)),
        )
        .expect("failed to construct w17 client?!?");

        let attributes = Attributes::new().set("temperature", 21.5);

        println!("Setting temperatur to: {:?}", attributes);

        let result = run_one(hass.set_state("climate.lounge_wandthermostat", attributes));

        assert!(result.is_ok());
        println!("state: {:?}", result.unwrap());
    }

    #[test]
    fn call_service() {
        let hass = HomeAssistant::new(
            "https://hub.w17.io",
            Some(HomeAssistantConfiguration::new().set_verify_certs(false)),
        )
        .expect("failed to construct w17 client?!?");
        let attrs = Attributes::new()
            .set("payload", "foo")
            .set("topic", "foo")
            .set("retain", false);
        let fut = hass.call_service("mqtt", "publish", Some(attrs));
        run_one(fut).unwrap();
    }
}
