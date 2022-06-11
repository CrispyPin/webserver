use std::io::prelude::*;
use std::{fs, env};
use std::net::{TcpStream, TcpListener};


fn main() {
	let args: Vec<String> = env::args().collect();

	let host = if args.len() < 2 {
		"127.0.0.1:55566"
	} else {
		&args[1]
	};
	println!("Starting server on {:?}...\n", &host);

	let listener = TcpListener::bind(host).expect("Could not bind to address");

	for stream in listener.incoming() {
		if let Ok(stream) = stream {
			handle_connection(stream);
		}
		else {
			println!("Error with incoming stream: {}", stream.err().unwrap());
		}
	}
}


fn handle_connection(mut stream: TcpStream) {
	let mut buffer = vec![0; 2048];
	let size = stream.read(&mut buffer).unwrap();
	buffer.resize(size, 0);

	send_file(&mut stream, "src/main.rs");
	
	let peer_addr = stream.peer_addr().unwrap();
	println!("Received {} bytes from {}\n\n[{}]\n", size, peer_addr,
		String::from_utf8_lossy(&buffer)
		.escape_debug()
		.collect::<String>()
		.replace("\\r\\n", "\n")
		.replace("\\n", "\n")
	);
}

fn send_file(stream: &mut TcpStream, path: &str) {
	let contents = fs::read_to_string(path).unwrap() + "\n\n";
	
	let response = format!(
		"HTTP/1.1 200 OK\nContent-Length: {}\n\n{}",
		contents.len(),
		contents
	);
	stream.write_all(response.as_bytes()).unwrap();
	stream.flush().unwrap();
}
