use std::io::prelude::*;
use std::net::{TcpListener, TcpStream};
use std::{env, fs};

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
		match stream {
			Ok(stream) => handle_connection(stream),
			Err(err) => println!("Error with incoming stream: {}", err),
		}
	}
}

fn handle_connection(mut stream: TcpStream) {
	let mut buffer = vec![0; 2048];
	let size = if let Ok(size) = stream.read(&mut buffer) {
		size
	} else {
		return;
	};

	buffer.resize(size, 0);

	let request = String::from_utf8_lossy(&buffer);

	let peer_addr = stream.peer_addr().ok();

	println!(
		"Received {} bytes from {:?}\n\n[{}]",
		size,
		peer_addr,
		request
			.escape_debug()
			.collect::<String>()
			.replace("\\r\\n", "\n")
			.replace("\\n", "\n")
	);

	let mut request = request.into_owned();
	while request.contains("..") {
		request = request.replace("..", "");
	}

	if request.starts_with("GET") {
		if let Some(file) = request.split_whitespace().nth(1) {
			let path = format!("./{}", file);
			send_file(&mut stream, &path);
			return;
		}
	}
	let response = "HTTP/1.1 400 BAD REQUEST\r\n\r\n";
	stream
		.write_all(response.as_bytes())
		.unwrap_or_else(|_| println!("failed to respond"));
	stream
		.flush()
		.unwrap_or_else(|_| println!("failed to respond"));
}

fn send_file(stream: &mut TcpStream, path: &str) {
	if let Ok(text) = fs::read_to_string(path) {
		let contents = text + "\n\n";
		let response = format!(
			"HTTP/1.1 200 OK\r\nContent-Type: {}; charset=UTF-8\r\nContent-Length: {}\r\n\r\n{}",
			if path.ends_with(".html") {
				"text/html"
			} else {
				"text/plain"
			},
			contents.len(),
			contents
		);
		stream
			.write_all(response.as_bytes())
			.unwrap_or_else(|_| println!("failed to respond"));
	} else {
		eprintln!("File does not exist: {}", path);
		let response = format!("HTTP/1.1 404 NOT FOUND\r\nContent-Type: text/plain; charset=UTF-8\r\nContent-Length: {}\r\n\r\n{}", path.len(), path);
		stream
			.write_all(response.as_bytes())
			.unwrap_or_else(|_| println!("failed to respond with 404"));
	};
	stream
		.flush()
		.unwrap_or_else(|_| println!("failed to respond"));
}
