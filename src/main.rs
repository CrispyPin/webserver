use std::{net::{TcpStream, TcpListener}, io::prelude::*, fs, env};


fn main() {
	let args: Vec<String> = env::args().collect();

	let host = if args.len() < 2 {
		"127.0.0.1:6666"
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
	let mut buffer = vec![0; 1024];
	let size = stream.read(&mut buffer).unwrap();
	buffer.resize(size, 0);	

	let contents = fs::read_to_string("src/main.rs").unwrap() + "\n\n";
	
	let response = format!(
		"HTTP/1.1 200 OK\nContent-Length: {}\n\n{}",
		contents.len() + buffer.len(),
		contents
	);
	stream.write_all(response.as_bytes()).unwrap();
	stream.write_all(&buffer).unwrap();
	stream.flush().unwrap();

	let peer_addr = stream.peer_addr().unwrap();
	println!("Received {} bytes from {}\n\n{}\n", size, peer_addr,
		String::from_utf8_lossy(&buffer)
		.escape_debug()
		.collect::<String>()
		.replace("\\r\\n", "\n")
		.replace("\\n", "\n")
	);
}
