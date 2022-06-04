use std::{net::{TcpStream, TcpListener}, io::prelude::*, fs};


fn main() {
	let listener = TcpListener::bind("192.168.0.108:25585").unwrap();

	for stream in listener.incoming() {
		let stream = stream.unwrap();
		println!("IP:  {}\n", stream.peer_addr().unwrap());

		handle_connection(stream);
	}
}


fn handle_connection(mut stream: TcpStream) {
	let mut buffer = vec![0; 1024];
	let size = stream.read(&mut buffer).unwrap();
	buffer.resize(size, 0);	
	let contents = fs::read_to_string("src/main.rs").unwrap();
	
	let response = format!(
		"HTTP/1.1 200 OK\nContent-Length: {}\n\n{}",
		contents.len() + buffer.len(),
		contents
	);
	
	stream.write(response.as_bytes()).unwrap();
	stream.write(&buffer).unwrap();
	stream.flush().unwrap();
	println!("read {} bytes:\n{}", size,
		String::from_utf8_lossy(&buffer)
		.escape_debug()
		.collect::<String>()
		.replace("\\r\\n", "\n")
		.replace("\\n", "\n")
	);
}
