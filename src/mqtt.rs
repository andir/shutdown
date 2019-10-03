use futures::future::Future;
use futures::sink::Sink;
use futures::stream::Stream;
use futures::sync::mpsc::{
    channel,
    Receiver,
    //RecvError,
    SendError,
    Sender,
};
use tokio::prelude::*;

use rumqtt::{MqttClient, MqttOptions, Notification, ReconnectOptions};

#[derive(Debug)]
pub enum Error {
    SendingFailed(SendError<OpCode>),
    //    ReceivingFailed(RecvError),
    MosquittoConnectError(rumqtt::error::ConnectError),
    ThreadJoinError,
}

impl From<rumqtt::error::ConnectError> for Error {
    fn from(e: rumqtt::error::ConnectError) -> Self {
        Error::MosquittoConnectError(e)
    }
}

type Topic = String;
type Value = String;

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum OpCode {
    MessageReceived((Topic, Value)),
    Subscribe(Topic),
    Unsubscribe(Topic),
    Publish((Topic, Value)),
    Shutdown,
    ShutdownComplete,
}

pub struct MqttConnection {
    event_emitter: Sender<OpCode>,
    event_receiver: Receiver<OpCode>,
}

impl MqttConnection {
    pub fn new() -> ((Sender<OpCode>, Receiver<OpCode>), Self) {
        let (inner_sender, outer_receiver) = channel(512);
        let (outer_sender, inner_receiver) = channel(512);

        let m = MqttConnection {
            event_emitter: inner_sender,
            event_receiver: inner_receiver,
        };

        ((outer_sender, outer_receiver), m)
    }

    pub fn run(self, broker: &str, port: u16) -> Result<()> {
        let reconnect_options = ReconnectOptions::Always(5);
        let mqtt_options = MqttOptions::new("space-shutdown", broker, port)
            .set_keep_alive(10)
            .set_reconnect_opts(reconnect_options)
            .set_clean_session(false);

        let (mut mqtt_client, notifications) = MqttClient::start(mqtt_options)?;

        let rx = self.event_receiver;
        let mut loop_client = mqtt_client.clone();
        let ht = std::thread::spawn(move || {
            tokio::run(rx.for_each(move |msg| {
                match msg {
                    OpCode::Subscribe(topic) => {
                        println!("subscribing: {}", topic);
                        loop_client
                            .subscribe(&topic, mqtt311::QoS::AtLeastOnce)
                            .expect("Failed to subscribe");
                        println!("subscribed");
                    }
                    OpCode::Publish((topic, value)) => {
                        loop_client
                            .publish(topic, mqtt311::QoS::AtLeastOnce, false, value.as_bytes())
                            .expect("failed to publish");
                    }
                    e => println!("Unimplemented event received: {:?}", e),
                };
                Ok(())
            }))
        });

        for notification in notifications {
            match notification {
                Notification::Publish(msg) => {
                    println!("Publish: {:?}", msg);
                    let payload = match std::str::from_utf8(&msg.payload) {
                        Ok(v) => v,
                        Err(e) => {
                            println!("failed to decode utf8 from payload: {}", e);
                            continue;
                        }
                    };

                    let fut = self
                        .event_emitter
                        .clone()
                        .send(OpCode::MessageReceived((
                            msg.topic_name,
                            payload.to_string(),
                        )))
                        .map(|_| ())
                        .map_err(|_| ());
                    tokio::run(futures::lazy(move || fut));
                }
                o => {
                    println!("Unhandled notification: {:?}", o);
                }
            }
        }
        ht.join().map_err(|_e| Error::ThreadJoinError)?;

        Ok(())
    }
}
