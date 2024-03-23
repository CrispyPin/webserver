use std::{
	env,
	fs::{self, File},
	io::{Read, Write},
	net::{TcpListener, TcpStream},
	path::{Path, PathBuf},
};

mod http;
use http::{Content, Method, Request, Response, Status};

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
		"Received {} bytes from {:?}\n=======\n{}=======\n\n",
		size,
		peer_addr,
		request
			.escape_debug()
			.collect::<String>()
			.replace("\\r\\n", "\n")
			.replace("\\n", "\n")
	);

	let request = Request::parse(&request);

	let response;

	if let Some(request) = request {
		let head_only = request.method == Method::Head;
		let path = request.path.clone();
		response = get_file(request)
			.map(|content| Response::new(Status::Ok).with_content(content))
			.unwrap_or_else(|| {
				Response::new(Status::NotFound)
					.with_content(Content::text(format!("FILE NOT FOUND - '{}'", path)))
			})
			.format(head_only);
	} else {
		response = Response::new(Status::BadRequest).format(false);
	}

	stream
		.write_all(&response)
		.unwrap_or_else(|_| println!("failed to respond"));
	stream
		.flush()
		.unwrap_or_else(|_| println!("failed to respond"));
}

fn get_file(request: Request) -> Option<Content> {
	let path = PathBuf::from(format!("./{}", &request.path))
		.canonicalize()
		.ok()?;
	if path.strip_prefix(env::current_dir().unwrap()).is_err() {
		return None;
	}

	if path.is_dir() {
		let index_file = path.join("index.html");
		if index_file.is_file() {
			Some(Content::html(fs::read_to_string(index_file).ok()?))
		} else {
			generate_index(&request.path, &path)
		}
	} else if path.is_file() {
		let ext = path.extension().unwrap_or_default().to_str()?;
		let mut buf = Vec::new();
		File::open(&path).ok()?.read_to_end(&mut buf).ok()?;
		Some(Content::file(ext, buf))
	} else {
		None
	}
}

fn generate_index(relative_path: &str, path: &Path) -> Option<Content> {
	let list = path
		.read_dir()
		.ok()?
		.flatten()
		.filter_map(|e| {
			let target = e.file_name().to_str()?.to_string();
			let mut s = format!(
				"	<li><a href=\"{}\"> {}",
				PathBuf::from(relative_path).join(&target).display(),
				target
			);
			if e.file_type().ok()?.is_dir() {
				s.push('/');
			}
			s.push_str("</a></li>\n");
			Some(s)
		})
		.fold(String::new(), |mut content, entry| {
			content.push_str(&entry);
			content
		});
	let page = format!(
		r#"<!DOCTYPE html>
<html lang="en">
<head>
	<meta charset="UTF-8">
	<meta name="viewport" content="width=device-width, initial-scale=1.0">
	<title>Index of {relative_path}</title>
</head>
<body>
	<h3>Index of {relative_path}</h3>
	<ul>
	<li><a href="..">../</a></li>
	{list}
	</ul>
</body>
</html>"#,
	);
	Some(Content::html(page))
}
