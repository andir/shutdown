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
use mosquitto_client::{MosqMessage, Mosquitto};
use tokio::prelude::*;

#[derive(Debug)]
pub enum Error {
    SendingFailed(SendError<OpCode>),
    //    ReceivingFailed(RecvError),
    MosquittoError(mosquitto_client::Error),
    ThreadJoinError,
}

impl From<mosquitto_client::Error> for Error {
    fn from(e: mosquitto_client::Error) -> Self {
        Error::MosquittoError(e)
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

    pub fn run(self, broker: &str, port: u32) -> Result<()> {
        let m = Mosquitto::new("space-shutdown");
        m.connect(broker, port)?;
        println!("connected!");

        let mut mc = m.callbacks(self.event_emitter);
        mc.on_message(|tx, msg| {
            //            println!("msg: {}={:?}", msg.topic(), msg.payload());
            let payload = match std::str::from_utf8(msg.payload()) {
                Ok(v) => v,
                Err(e) => {
                    println!("failed to decode utf8 from payload: {}", e);
                    return;
                }
            };

            let topic = msg.topic();
            let tx = tx.clone();
            let fut = tx
                .send(OpCode::MessageReceived((
                    topic.to_string(),
                    payload.to_string(),
                )))
                .map(|_| ())
                .map_err(|_| ());
            tokio::run(futures::lazy(move || fut));
        });

        let rx = self.event_receiver;
        let mt = m.clone();
        let ht = std::thread::spawn(move || {
            tokio::run(rx.for_each(move |msg| {
                match msg {
                    OpCode::Subscribe(topic) => {
                        println!("subscribing: {}", topic);
                        mt.subscribe(&topic, 0).expect("Failed to subscribe");
                        println!("subscribed");
                    }
                    e => println!("Unimplemented event received: {:?}", e),
                };
                Ok(())
            }))
        });

        let mt = m.clone();
        let jh = std::thread::spawn(move || {
            mt.loop_until_disconnect(200)
                .expect("Failed to run loop until disconnect");
            println!("MQTT loop disconnected");
        });

        jh.join().map_err(|_e| Error::ThreadJoinError)?;
        ht.join().map_err(|_e| Error::ThreadJoinError)?;

        Ok(())
    }
}
