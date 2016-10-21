// Copyright 2015 Justin Noah, All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
extern crate bincode;
extern crate chrono;
extern crate docopt;
extern crate env_logger;
#[macro_use]
extern crate log;
extern crate rand;
extern crate rustc_serialize;
extern crate rusqlite;
extern crate serde;

use std::net::UdpSocket;
use std::path::Path;
use std::thread;
use std::time::Duration;

use docopt::Docopt;
use rusqlite::SqliteConnection;

use config::load_config;
use handler::handle_response;

mod config;
mod handler;
mod parse_packets;


// Initialize the database
fn init_db(path: &Path) {
    let conn = SqliteConnection::open(&path).unwrap();
    conn.execute("
        CREATE TABLE IF NOT EXISTS torrent (
            info_hash   TEXT,
            ip          INTEGER,
            port        INTEGER,
            peer_id     TEXT,
            remaining   INTEGER,
            last_active INTEGER,
            PRIMARY KEY (info_hash, ip, port, peer_id)
        );",
        &[]
    ).unwrap();
    debug!("Connection to {:?} has been made", path);
}

static USAGE: &'static str = "
Usage: rtracker [-c <conf>]
       rtracker (--help)

Options:
    -h, --help          Show this message
    -c, --conf=<conf>   Configuration File [default: ]
";

#[derive(RustcDecodable)]
struct Args {
    flag_conf: String,
}

fn main() {
    env_logger::init().unwrap();
    trace!("Logging initialized!");

    // parse commandline args
    let args: Args = Docopt::new(USAGE)
                            .and_then(|d| d.decode())
                            .unwrap_or_else(|e| e.exit());

    let (ip, port) = load_config(args.flag_conf);
    let ip_string = format!("{}:{}", ip, port);

    let database_path = Path::new("file::memory:?cache=shared");

    // Let's first initialize the database.
    let _ = init_db(database_path);
    let sock = match UdpSocket::bind(&ip_string[..]) {
        Ok(s) => s,
        Err(e) => panic!("{}", e),
    };

    info!("Listening on: {}", &ip_string);

    // Spawn the database pruning thread
    thread::spawn(move|| {
        loop {
            // Every 31min (default is 30min, this allows for some delay)
            let prune_delay = Duration::new(31 * 60 as u64, 0);
            thread::sleep(prune_delay);

            // Prune the database
            debug!("Prune the database!");
            SqliteConnection::open(&database_path).unwrap().execute(
                "DELETE FROM torrent
                WHERE (strftime('%s','now') - last_active) > 1860;",
                &[]
            ).unwrap();
        }
    });

    loop {
        debug!("Init a 2048 byte array");
        let mut buf = [0u8; 2048];
        debug!("Read");
        let (amt, src) = sock.recv_from(&mut buf).unwrap();
        debug!("Clone Socket");
        let tsock = sock.try_clone().unwrap();
        debug!("buf.to_vec");
        let mut b: Vec<u8> = buf.to_vec();
        debug!("Trucate vec at {}", amt);
        b.truncate(amt);
        debug!("Spawn a new thread to handle the packet");
        thread::spawn(move|| {
            debug!("Thread: sql connect");
            let conn = SqliteConnection::open(&database_path).unwrap();
            handle_response(tsock, &src, b, &conn);
            let _ = conn.close();
            debug!("Thread: done");
        });
    }
}
