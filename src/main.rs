use futures::future::lazy;
use futures::future::Future;
use futures::sink::Sink;
use futures::stream::Stream;
use futures::sync::oneshot;
use std::sync::*;

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
    interrupter: Arc<Mutex<Option<oneshot::Sender<()>>>>,
    door_topic: String,
    delay: std::time::Duration,
    sender: futures::sync::mpsc::Sender<OpCode>,
    shutdown_messages: Vec<ShutdownMessage>,
}

impl AutoShutdown {
    fn new(
        topic: &str,
        delay: std::time::Duration,
        sender: futures::sync::mpsc::Sender<OpCode>,
        shutdown_messages: Vec<ShutdownMessage>,
    ) -> Self {
        AutoShutdown {
            interrupter: Arc::new(Mutex::new(None)),
            door_topic: topic.to_string(),
            delay,
            sender,
            shutdown_messages,
        }
    }

    fn shutdown_futures(&self) -> Box<impl Future<Item = (), Error = ()>> {
        let one_future = |msg: ShutdownMessage| {
            let sender = self.sender.clone();
            sender
                .send(OpCode::Publish((msg.topic, msg.value)))
                .map(|_| ())
                .map_err(|_| ())
                .then(|_| Ok(()))
        };

        Box::new(
            futures::future::join_all(
                self.shutdown_messages
                    .iter()
                    .cloned()
                    .map(one_future)
                    .collect::<Vec<_>>(),
            )
            .map(|_| ()),
        )
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
                                        futs
                                    })
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
    let door_topic = "w17/doorfake/lock/state";
    let delay = std::time::Duration::from_millis(1000);
    let ((tx, rx), m) = mqtt::MqttConnection::new();

    let shutdown_messages = vec![
        ShutdownMessage::new("/foo", "bar")
    ];

    let auto_shutdown = AutoShutdown::new(door_topic, delay, tx.clone(), shutdown_messages);
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
