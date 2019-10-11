#[macro_use]
extern crate serde;

use futures::future::lazy;
use futures::future::Future;
use futures::sink::Sink;
use futures::stream::Stream;
use futures::sync::oneshot;
use std::sync::*;

mod hass;
mod mqtt;

use mqtt::OpCode;

#[derive(Clone)]
struct ShutdownMessage {
    topic: String,
    value: String,
}

impl ShutdownMessage {
    fn new(topic: &str, value: &str) -> Self {
        Self {
            topic: topic.to_string(),
            value: value.to_string(),
        }
    }
}

struct AutoShutdown {
    hass: hass::HomeAssistant,
    interrupter: Arc<Mutex<Option<oneshot::Sender<()>>>>,
    door_topic: String,
    delay: std::time::Duration,
    sender: futures::sync::mpsc::Sender<OpCode>,
    shutdown_messages: Vec<ShutdownMessage>,
}

impl AutoShutdown {
    fn new(
        hass: hass::HomeAssistant,
        topic: &str,
        delay: std::time::Duration,
        sender: futures::sync::mpsc::Sender<OpCode>,
        shutdown_messages: Vec<ShutdownMessage>,
    ) -> Self {
        AutoShutdown {
            hass,
            interrupter: Arc::new(Mutex::new(None)),
            door_topic: topic.to_string(),
            delay,
            sender,
            shutdown_messages,
        }
    }

    fn shutdown_futures(&self) -> Box<dyn Future<Item = (), Error = ()> + Send> {
        let futs = vec![
            self.shutdown_temperature_futures(),
            self.shutdown_mqtt_futures(),
        ];
        Box::new(futures::future::join_all(futs).map(|_| ()))
    }

    fn shutdown_temperature_futures(&self) -> Box<dyn Future<Item = (), Error = ()> + Send> {
        let thermoststats = vec![
            ("workshop_wandthermostat", 15.0),
            ("lounge_wandthermostat", 18.0),
            ("kitchen_wandthermostat", 18.0),
        ];

        let mut futures = vec![];

        for (room, temp) in thermoststats.into_iter() {
            futures.push(
                hass::set_temperature(&self.hass, format!("climate.{}", room), temp)
                    .map(move |r| println!("set temperatur in {} to {}: {:?}", room, temp, r))
                    .map_err(|e| println!("failed to set temperature: {:?}", e)),
            );
        }

        let fut = futures::future::join_all(futures).map(|_| ());

        Box::new(fut)
    }

    fn shutdown_mqtt_futures(&self) -> Box<dyn Future<Item = (), Error = ()> + Send> {
        let one_future = |msg: ShutdownMessage| {
            let sender = self.sender.clone();
            sender
                .send(OpCode::Publish((msg.topic.clone(), msg.value.clone())))
                .map(move |_| println!("published {} {}", msg.topic, msg.value))
                .map_err(|_| ())
                .then(|_| Ok(()))
        };
        let mqtt_futures = futures::future::join_all(
            self.shutdown_messages
                .iter()
                .cloned()
                .map(one_future)
                .collect::<Vec<_>>(),
        )
        .map(|_| ());

        Box::new(mqtt_futures)
    }

    fn handle_msg(&self, msg: mqtt::OpCode) {
        match msg {
            OpCode::MessageReceived((topic, value)) => {
                println!("<msg: {} {}", topic, value);
                if topic == self.door_topic {
                    let a = Arc::clone(&self.interrupter);
                    let mut it = a.lock().expect("Mutex poisoned");
                    match ((value == "1"), &*it) {
                        // door is unlocked and timer is running, abort the timer by sending
                        // interrupt
                        (false, Some(_)) => {
                            println!("Stopping timer");

                            // Move the sender out of the mutex so we can use it
                            let it = std::mem::replace(&mut *it, None);

                            // Unwrap is safe here as we checked that in the match condition a few
                            // lines earlier.
                            it.unwrap().send(()).expect("failed to send");
                        }
                        // door is locked and not timer is running, start one and assign
                        // interrupter
                        (true, None) => {
                            println!("spawning timer!");
                            let (sender, receiver) = oneshot::channel();
                            *it = Some(sender);

                            let futs = self.shutdown_futures();
                            let it_clone = Arc::clone(&self.interrupter);
                            let d =
                                tokio::timer::Delay::new(std::time::Instant::now() + self.delay)
                                    .map_err(|_| ())
                                    .and_then(move |_| {
                                        let mut it = it_clone.lock().expect("Mutex poisoned");
                                        println!("timer expired");
                                        *it = None;
                                        tokio::spawn(futs)
                                    })
                                    .map(|_| println!("futures executed"))
                                    .map_err(|_| ());

                            let receiver = receiver.map_err(|_| ());
                            let fut = d.select(receiver);
                            tokio::spawn(lazy(|| fut.then(|_| Ok(()))));
                        }
                        // all other cases: We do not need to do much here. Mostly just if the door
                        // is already locked and got locked again (how?) and unlocked and gets
                        // unlocked again…
                        (state, it) => {
                            println!("Ignoring state {} {:?}", state, it);
                        }
                    }
                };
            }
            e => println!("unhandled message: {:?}", e),
        };
    }
}

fn main() {
    env_logger::init();

    let hass = hass::HomeAssistant::new(
        //"https://172.20.64.212",
        "https://hub.w17.io",
        Some(hass::HomeAssistantConfiguration::new().set_verify_certs(false)),
    )
    .unwrap();

    let door_topic = "w17/doorfake/lock/state";
    let delay = std::time::Duration::from_millis(1_0 * 60);
    let ((tx, rx), m) = mqtt::MqttConnection::new();

    let shutdown_messages = vec![
        ShutdownMessage::new("w17/kitchen/bear/set", "0"),
        ShutdownMessage::new("w17/kitchen/amp/set", "0"),
        ShutdownMessage::new("w17/kitchen/tv/power/set", "0"),
        ShutdownMessage::new("w17/lounge/amp/set", "0"),
        ShutdownMessage::new("w17/lounge/video/set", "0"),
        ShutdownMessage::new("w17/lounge/printer/set", "0"),
        ShutdownMessage::new("w17/lounge/leds/3dprinter/set", "0"),
        ShutdownMessage::new("w17/lounge/leds/auditorium/set", "0"),
        ShutdownMessage::new("w17/lounge/leds/beamer/set", "0"),
    ];

    let auto_shutdown = AutoShutdown::new(hass, door_topic, delay, tx.clone(), shutdown_messages);
    std::thread::spawn(|| {
        println!("connecting!");
        m.run("mqtt.w17.io", 1883).unwrap();
    });

    let fut = tx
        .send(mqtt::OpCode::Subscribe(door_topic.to_string()))
        .map_err(|e| println!("error: {}", e))
        .map(|_| ())
        .and_then(move |_| {
            rx.for_each(move |msg| {
                auto_shutdown.handle_msg(msg);
                Ok(())
            })
        });

    tokio::run(fut);
}
