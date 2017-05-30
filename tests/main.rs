extern crate amq_protocol;
extern crate futures;
extern crate tokio_core;
extern crate lapin_futures as lapin;
extern crate postgres;

use amq_protocol::types::FieldTable;
use postgres::{Connection, TlsMode};
use futures::*;
use tokio_core::reactor::Core;
use tokio_core::net::TcpStream;
use lapin::client::*;
use lapin::channel::*;

const TEST_AMQP_HOST_PORT: &str = "127.0.0.1:5672";
const TEST_PG_URI: &str = "postgres://postgres@localhost";

#[test]
fn publishing_to_queue_works() {
  const TEST_QUEUE: &str = "pg_amqp_bridge_queue_1";
  const TEST_PG_CHANNEL: &str = "pgchannel1";

  let mut core = Core::new().unwrap();
  let handle = core.handle();
  let addr = TEST_AMQP_HOST_PORT.parse().unwrap();

  let pg_conn = Connection::connect(TEST_PG_URI, TlsMode::None).unwrap();

  let _ = core.run(
    TcpStream::connect(&addr, &handle)
    .and_then(|stream| Client::connect(stream, &ConnectionOptions::default()) )
    .and_then(|client| client.create_channel())
    .and_then(|channel|{
      let ch1 = channel.clone();
      let ch2 = channel.clone();
      channel.queue_declare(TEST_QUEUE, &QueueDeclareOptions::default(), FieldTable::new())
      .and_then(move |_| channel.basic_consume(TEST_QUEUE, "my_consumer_1", &BasicConsumeOptions::default()))
      .and_then(move |stream| {
        let _ = pg_conn.execute(format!("NOTIFY {}, '{}|Queue test'", TEST_PG_CHANNEL, TEST_QUEUE).as_str(), &[]);
        stream.into_future().map_err(|(err, _)| err) 
      })
      .and_then(move |(message, _)| {
        let msg = message.unwrap();
        assert_eq!(msg.data, b"\"Queue test\"");
        ch1.basic_ack(msg.delivery_tag) 
      })
      .then(move |_| ch2.queue_delete(TEST_QUEUE, &QueueDeleteOptions::default()))
    })
  );
}

#[test]
fn publishing_to_direct_exchange_works() {
  const TEST_QUEUE: &str = "pg_amqp_bridge_queue_2";
  const TEST_EXCHANGE: &str = "pg_amqp_bridge_direct_exchange";
  const TEST_PG_CHANNEL: &str = "pgchannel2";

  let mut core = Core::new().unwrap();
  let handle = core.handle();
  let addr = TEST_AMQP_HOST_PORT.parse().unwrap();

  let pg_conn = Connection::connect(TEST_PG_URI, TlsMode::None).unwrap();

  let _ = core.run(
    TcpStream::connect(&addr, &handle)
    .and_then(|stream| Client::connect(stream, &ConnectionOptions::default()) )
    .and_then(|client| client.create_channel())
    .and_then(|channel|{
      let ch1 = channel.clone();
      let ch2 = channel.clone();
      channel.queue_declare(TEST_QUEUE, &QueueDeclareOptions::default(), FieldTable::new())
      .and_then(move |_| 
        channel.exchange_declare(TEST_EXCHANGE, "direct", 
                                          &ExchangeDeclareOptions{
                                            passive: false, durable: false, 
                                            auto_delete: true, internal: false, 
                                            nowait: false
                                          }, FieldTable::new())
        .and_then(move |_| channel.queue_bind(TEST_QUEUE, TEST_EXCHANGE, "test_direct_key", &QueueBindOptions::default(), FieldTable::new()))
      )
      .and_then(move |_| ch1.basic_consume(TEST_QUEUE, "my_consumer_2", &BasicConsumeOptions::default()))
      .and_then(move |stream| {
        let _ = pg_conn.execute(format!("NOTIFY {}, 'test_direct_key|Direct exchange test'", TEST_PG_CHANNEL).as_str(), &[]);
        stream.into_future().map_err(|(err, _)| err) 
      })
      .and_then(move |(message, _)| {
        let msg = message.unwrap();
        assert_eq!(msg.data, b"\"Direct exchange test\"");
        ch2.basic_ack(msg.delivery_tag) 
        .then(move |_| ch2.queue_delete(TEST_QUEUE, &QueueDeleteOptions::default()))
      })
    })
  );
}

#[test]
#[ignore] //Works intermittently
fn publishing_to_topic_exchange_works() {
  const TEST_QUEUE_1: &str = "pg_amqp_bridge_queue_3";
  const TEST_QUEUE_2: &str = "pg_amqp_bridge_queue_4";
  const TEST_EXCHANGE: &str = "pg_amqp_bridge_topic_exchange";
  const TEST_PG_CHANNEL: &str = "pgchannel3";

  let mut core = Core::new().unwrap();
  let handle = core.handle();
  let addr = TEST_AMQP_HOST_PORT.parse().unwrap();

  let _ = core.run(
    TcpStream::connect(&addr, &handle)
    .and_then(|stream| Client::connect(stream, &ConnectionOptions::default()) )
    .and_then(|client| client.create_channel())
    .and_then(|channel|{
      let ch1 = channel.clone();
      let ch2 = channel.clone();
      let ch3 = channel.clone();
      channel.queue_declare(TEST_QUEUE_1, &QueueDeclareOptions::default(), FieldTable::new())
      .and_then(move |_| 
        channel.queue_declare(TEST_QUEUE_2, &QueueDeclareOptions::default(), FieldTable::new())
        .and_then(move |_| 
          channel.exchange_declare(TEST_EXCHANGE, "topic", 
                                                &ExchangeDeclareOptions{
                                                  passive: false, durable: false, 
                                                  auto_delete: true, internal: false, 
                                                  nowait: false
                                                }, FieldTable::new())
          .and_then(move |_| 
            channel.queue_bind(TEST_QUEUE_1, TEST_EXCHANGE, "*.critical", &QueueBindOptions::default(), FieldTable::new())
            .and_then(move |_| channel.queue_bind(TEST_QUEUE_2, TEST_EXCHANGE, "*.info", &QueueBindOptions::default(), FieldTable::new()))
          )
        )
      )
      .and_then(move |_| 
        ch1.basic_consume(TEST_QUEUE_1, "my_consumer_3", &BasicConsumeOptions::default())
        .and_then(move |stream| {
          let pg_conn = Connection::connect(TEST_PG_URI, TlsMode::None).unwrap();
          let _ = pg_conn.execute(format!("NOTIFY {}, 'kern.critical|Kernel critical message'", TEST_PG_CHANNEL).as_str(), &[]);
          stream.into_future().map_err(|(err, _)| err) 
        })
        .and_then(move |(message, _)| {
          let msg = message.unwrap();
          assert_eq!(msg.data, b"\"Kernel critical message\"");
          ch1.basic_ack(msg.delivery_tag) 
        })
      )
      .and_then(move |_| 
        ch2.basic_consume(TEST_QUEUE_2, "my_consumer_4", &BasicConsumeOptions::default())
        .and_then(move |stream| {
          let pg_conn = Connection::connect(TEST_PG_URI, TlsMode::None).unwrap();
          let _ = pg_conn.execute(format!("NOTIFY {}, 'user.info|User info message'", TEST_PG_CHANNEL).as_str(), &[]);
          stream.into_future().map_err(|(err, _)| err)
        })
        .and_then(move |(message, _)| {
          let msg = message.unwrap();
          assert_eq!(msg.data, b"\"Anon info message\"");
          ch2.basic_ack(msg.delivery_tag) 
        })
      )
      .then(move |_| 
        ch3.queue_delete(TEST_QUEUE_1, &QueueDeleteOptions::default())
        .then(move |_| ch3.queue_delete(TEST_QUEUE_2, &QueueDeleteOptions::default()))
       )
    })
  );
}
