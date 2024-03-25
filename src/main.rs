use std::{
	env,
	fs::{self, File},
	io::{BufReader, Read, Seek, Write},
	net::{TcpListener, TcpStream},
	path::{Path, PathBuf},
	thread,
};

mod http;
use http::{Content, Method, Request, RequestRange, Response, Status};

fn main() {
	let args: Vec<String> = env::args().collect();

	let host = if args.len() < 2 {
		"127.0.0.1:55566"
	} else {
		&args[1]
	};
	println!("Starting server on {:?}...\n", &host);

	let listener = TcpListener::bind(host).expect("Could not bind to address");

	let mut threads = Vec::new();

	for stream in listener.incoming() {
		match stream {
			Ok(stream) => threads.push(thread::spawn(|| handle_connection(stream))),
			Err(err) => println!("Error with incoming stream: {}", err),
		}
		threads = threads.into_iter().filter(|j| !j.is_finished()).collect();
		println!("{} connections open", threads.len());
	}
}

fn handle_connection(mut stream: TcpStream) {
	const MAX_REQUEST_SIZE: usize = 1024 * 4;
	let Ok(client_ip) = stream.peer_addr() else {
		return;
	};
	println!("[{client_ip}] new connection");

	let mut buffer = Vec::with_capacity(2048);
	loop {
		let mut b = vec![0; 512];
		let Ok(size) = stream.read(&mut b) else {
			println!("[{client_ip}] connection broken");
			return;
		};
		if size == 0 {
			println!("[{client_ip}] connection closed by client");
			return;
		}
		b.truncate(size);
		buffer.extend_from_slice(&b);

		if buffer.len() > MAX_REQUEST_SIZE {
			println!("[{client_ip}] request over {MAX_REQUEST_SIZE} bytes, closing connection");
			return;
		}
		if buffer.ends_with(b"\r\n\r\n") {
			let request = String::from_utf8_lossy(&buffer).to_string();

			println!("[{client_ip}] received {} bytes", buffer.len());
			// println!(
			// 	"=======\n{}=======\n\n",
			// 	request
			// 		.escape_debug()
			// 		.collect::<String>()
			// 		.replace("\\r\\n", "\n")
			// 		.replace("\\n", "\n")
			// );
			if handle_request(request, &mut stream) {
				println!("[{client_ip}] closing connection");
				return;
			}
			buffer.clear();
		}
	}
}

fn handle_request(request: String, stream: &mut TcpStream) -> bool {
	let Ok(client_ip) = stream.peer_addr() else {
		return true;
	};
	let request = Request::parse(&request);
	let response;
	let mut end_connection = true;

	if let Some(request) = request {
		println!("[{client_ip}] {} {}", request.method, request.path);
		let head_only = request.method == Method::Head;
		let path = request.path.clone();
		response = get_file(request)
			.map(|(content, end_of_file)| {
				end_connection = end_of_file;
				println!("[{client_ip}] sending file content");
				Response::new(Status::Ok).with_content(content)
			})
			.unwrap_or_else(|| {
				println!("[{client_ip}] file not found");
				Response::new(Status::NotFound)
					.with_content(Content::text(format!("404 NOT FOUND - '{}'", path)))
			})
			.format(head_only);
	} else {
		println!("[{client_ip}] bad request");
		response = Response::new(Status::BadRequest).format(false);
	}

	if stream.write_all(&response).is_err() || stream.flush().is_err() {
		println!("[{client_ip}] failed to send response");
	}
	end_connection
}

fn get_file(request: Request) -> Option<(Content, bool)> {
	const MAX_SIZE: usize = 1024 * 1024 * 8;

	let current_dir = env::current_dir().unwrap();

	let path = current_dir
		.join(request.path.strip_prefix('/')?)
		.canonicalize()
		.ok()?;

	if path
		.strip_prefix(current_dir.canonicalize().unwrap())
		.is_err()
	{
		println!("illegal path: {}", request.path);
		return None;
	}

	if path.is_dir() {
		let index_file = path.join("index.html");
		if index_file.is_file() {
			Some((Content::html(fs::read_to_string(index_file).ok()?), true))
		} else {
			generate_index(&request.path, &path).map(|c| (c, true))
		}
	} else if path.is_file() {
		let ext = path.extension().unwrap_or_default().to_str()?;
		let file = File::open(&path).ok()?;
		let size = file.metadata().ok()?.len() as usize;

		let mut buf = vec![0; MAX_SIZE];
		let mut reader = BufReader::new(file);
		let start_pos = match request.range {
			Some(RequestRange::From(p)) => p,
			Some(RequestRange::Full(start, _end)) => start,
			_ => 0,
		};
		reader
			.seek(std::io::SeekFrom::Start(start_pos as u64))
			.ok()?;

		let size_read = reader.read(&mut buf).ok()?;
		buf.truncate(size_read);
		let mut end_of_file = false;
		let range = if size_read < size {
			end_of_file = start_pos + size_read == size;
			Some((start_pos, start_pos + size_read - 1, size))
		} else {
			None
		};
		Some((Content::file(ext, buf).with_range(range), end_of_file))
	} else {
		None
	}
}

fn generate_index(relative_path: &str, path: &Path) -> Option<Content> {
	let mut dirs: Vec<_> = path
		.read_dir()
		.ok()?
		.flatten()
		.filter_map(|d| {
			let is_dir = d.file_type().ok()?.is_dir();
			d.file_name().to_str().map(|s| (s.to_owned(), is_dir))
		})
		.collect();
	dirs.sort_by(|(name_a, dir_a), (name_b, dir_b)| dir_b.cmp(dir_a).then(name_a.cmp(name_b)));
	let list = dirs
		.into_iter()
		.filter_map(|(name, is_dir)| {
			let mut s = format!(
				"		<li><a href=\"{}\">{}",
				PathBuf::from(relative_path).join(&name).display(),
				name
			);
			if is_dir {
				s.push('/');
			}
			s.push_str("</a></li>\n");
			Some(s)
		})
		.fold(String::new(), |mut content, entry| {
			content.push_str(&entry);
			content
		});
	let parent = if relative_path != "/" {
		r#"		<li><a href="..">../</a></li>"#
	} else {
		""
	};
	let page = format!(
		r#"<!DOCTYPE html>
<head>
	<meta charset="UTF-8">
	<title>Index of {relative_path}</title>
</head>
<body>
	<h3>Index of {relative_path}</h3>
	<ul>
{parent}{list}	</ul>
</body>
</html>"#,
	);
	Some(Content::html(page))
}
