extern crate url;
extern crate env_logger;
extern crate pg_amqp_bridge as bridge;
extern crate openssl;
extern crate postgres;
extern crate r2d2;
extern crate r2d2_postgres;

use std::env;
use std::thread;
use std::time::Duration;
use std::collections::HashMap;
use url::{Url};
use openssl::ssl::*;
use r2d2::{Pool, ManageConnection};
use r2d2_postgres::{TlsMode, PostgresConnectionManager};

#[derive(Debug, Clone)]
struct Config {
  postgresql_uri: String,
  amqp_uri: String,
  bridge_channels: String,
  delivery_mode: u8,
}

impl Config {
  fn new() -> Config {
    Config {
      postgresql_uri: env::var("POSTGRESQL_URI").expect("POSTGRESQL_URI environment variable must be defined"),
      amqp_uri: env::var("AMQP_URI").expect("AMQP_URI environment variable must be defined"),
      bridge_channels: env::var("BRIDGE_CHANNELS").expect("BRIDGE_CHANNELS environment variable must be defined"),
      delivery_mode:
        match env::var("DELIVERY_MODE").ok().as_ref().map(String::as_ref){
          None => 1,
          Some("NON-PERSISTENT") => 1,
          Some("PERSISTENT") => 2,
          Some(_) => panic!("DELIVERY_MODE environment variable can only be PERSISTENT or NON-PERSISTENT")
        }
    }
  }
}

fn main() {
  env_logger::init().unwrap();
  let config = Config::new();

  loop {
    let pool = wait_for_pg_connection(&config.postgresql_uri);
    // This functions spawns threads for each pg channel and waits for the threads to finish,
    // that only occurs when the threads die due to a pg connection error
    // and so if that happens the pg connection is retried and the bridge is started again.
    bridge::start(pool, &config.amqp_uri, &config.bridge_channels, &config.delivery_mode);
  }
}

fn wait_for_pg_connection(pg_uri: &String) -> Pool<PostgresConnectionManager> {
  println!("Attempting to connect to PostgreSQL..");

  let parsed_url = Url::parse(env::var("POSTGRESQL_URI").ok().as_ref().unwrap()).unwrap();
  let hash_query: HashMap<String, String> = parsed_url.query_pairs().into_owned().collect();
  let dbssl = match hash_query.get("sslmode") {
    None => "require",
    Some(mode) => mode
  };

  let mut builder = SslConnector::builder(::openssl::ssl::SslMethod::tls()).unwrap();
  match dbssl.to_lowercase().as_ref() {
      "disable" | "require" | "prefer" | "allow" => builder.set_verify(SslVerifyMode::empty()),
      _ => (), // by default we verify certs: it's like either verify-ca or verify-full, TBD
  }

  let negotiator = Box::new(::postgres::tls::openssl::OpenSsl::new().unwrap());
  let db_ssl_mode = match dbssl.to_lowercase().as_ref() {
      "require" | "verify-ca" | "verify-full" => TlsMode::Require(negotiator),
      // `disable`, `prefer`, and `allow` fall into here and will not try TLS.
      // Not totally correct: please use at least `require` for real use.
      _ => TlsMode::None,
  };

  let manager = PostgresConnectionManager::new(pg_uri.to_owned(), db_ssl_mode)
    .expect("Couldn't make postgres connection manager");

  let mut i = 1;
  while let Err(e) = manager.connect() {
    println!("{:?}", e);
    let time = Duration::from_secs(i);
    println!("Retrying the PostgreSQL connection in {:?} seconds..", time.as_secs());
    thread::sleep(time);
    i *= 2;
    if i > 32 { i = 1 };
  };
  println!("Connection to PostgreSQL successful");
  Pool::new(manager).expect("Couldn't make pool from pg connection manager")
}
