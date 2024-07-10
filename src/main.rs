use std::{
	env,
	fs::{self, File},
	io::{Read, Seek, Write},
	net::{TcpListener, TcpStream},
	path::{Path, PathBuf},
	thread,
	time::{Duration, SystemTime},
};

mod http;
use http::{Content, Method, Request, RequestRange, Response, Status};

const MAX_CONNECTIONS: usize = 256;

fn main() {
	let args: Vec<String> = env::args().collect();

	let host = if args.len() < 2 {
		"127.0.0.1:55566"
	} else {
		&args[1]
	};
	println!("Starting server on {:?}", &host);

	if args.len() > 2 {
		env::set_current_dir(&args[2]).expect("root dir specified must be valid path");
		println!("Set root dir to {}", &args[2]);
	}
	println!();

	let listener = TcpListener::bind(host).expect("Could not bind to address");

	let mut threads = Vec::new();

	for stream in listener.incoming() {
		match stream {
			Ok(stream) => threads.push(thread::spawn(|| handle_connection(stream))),
			Err(err) => println!("Error with incoming stream: {err}"),
		}
		threads.retain(|j| !j.is_finished());
		while threads.len() >= MAX_CONNECTIONS {
			threads.retain(|j| !j.is_finished());
			thread::sleep(Duration::from_millis(500));
			println!("Warning: maximum connections reached ({MAX_CONNECTIONS})")
		}
		println!("{} connections open", threads.len());
	}
}

fn handle_connection(mut stream: TcpStream) {
	const MAX_REQUEST_SIZE: usize = 1024 * 4;
	let Ok(client_ip) = stream.peer_addr() else {
		return;
	};
	println!("[{client_ip}] new connection at {}", formatted_time_now());

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
		if buffer.ends_with(b"\r\n\r\n") || buffer.ends_with(b"\n\n") {
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
			if handle_request(&request, &mut stream) {
				println!("[{client_ip}] closing connection");
				return;
			}
			buffer.clear();
		}
	}
}

fn handle_request(request: &str, stream: &mut TcpStream) -> bool {
	let Ok(socket_addr) = stream.peer_addr() else {
		return true;
	};
	let request = Request::parse(request);
	let response;
	let mut end_connection = true;

	if let Some(request) = request {
		let client_ip = format!("{socket_addr}");
		let client_ip = request.real_ip.as_ref().unwrap_or(&client_ip);
		println!("[{client_ip}] {}", request.user_agent);
		println!("[{client_ip}] {} {}", request.method, request.path);
		let head_only = request.method == Method::Head;
		let path = request.path.clone();
		response = get_file(&request)
			.map(|(content, end_of_file)| {
				end_connection = end_of_file;
				println!("[{client_ip}] sending file content");
				Response::new(Status::Ok).with_content(content)
			})
			.unwrap_or_else(|| {
				println!("[{client_ip}] file not found");
				Response::new(Status::NotFound)
					.with_content(Content::text(format!("404 NOT FOUND - '{path}'")))
			})
			.format(head_only);
	} else {
		println!("[{socket_addr}] bad request");
		response = Response::new(Status::BadRequest).format(false);
	}

	if stream.write_all(&response).is_err() || stream.flush().is_err() {
		println!("[{socket_addr}] failed to send response");
	}
	end_connection
}

fn get_file(request: &Request) -> Option<(Content, bool)> {
	const MAX_PARTIAL_PACKET_SIZE: usize = 1024 * 1024 * 8;

	let current_dir = env::current_dir().unwrap();

	let request_path = request.path.strip_prefix('/')?;
	let request_path_with_ext = format!("{request_path}.html");
	let path = fs::canonicalize(current_dir.join(request_path))
		.or_else(|_| fs::canonicalize(current_dir.join(request_path_with_ext)))
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
		let mut file = File::open(&path).ok()?;
		let file_size = file.metadata().ok()?.len() as usize;
		let mime_type = mime_type(ext);

		let buffer_size = if mime_type.is_some() {
			MAX_PARTIAL_PACKET_SIZE
		} else {
			file_size
		};
		let mut bytes = vec![0; buffer_size];
		let start_pos = match request.range {
			Some(RequestRange::From(p)) => p,
			Some(RequestRange::Full(start, _end)) => start,
			_ => 0,
		};
		file.seek(std::io::SeekFrom::Start(start_pos as u64)).ok()?;

		let size_read = file.read(&mut bytes).ok()?;
		bytes.truncate(size_read);
		let mut end_of_file = false;
		let range = if size_read < file_size {
			end_of_file = start_pos + size_read == file_size;
			Some((start_pos, start_pos + size_read - 1, file_size))
		} else {
			None
		};
		let mime_type = mime_type.unwrap_or_else(|| {
			if bytes.is_ascii() {
				"text/plain"
			} else {
				"application/octet-stream"
			}
		});
		Some((
			Content {
				mime_type,
				range,
				bytes,
			},
			end_of_file,
		))
	} else {
		None
	}
}

fn generate_index(relative_path: &str, path: &Path) -> Option<Content> {
	let mut items: Vec<_> = path
		.read_dir()
		.ok()?
		.flatten()
		.filter_map(|d| {
			let file_type = d.file_type().ok()?;
			if !(file_type.is_file() || file_type.is_dir()) {
				return None;
			}
			let size = if file_type.is_dir() {
				None
			} else {
				Some(d.metadata().ok()?.len())
			};

			d.file_name().to_str().map(|s| (s.to_owned(), size))
		})
		.collect();
	items.sort_by(|(name_a, size_a), (name_b, size_b)| {
		size_a
			.is_some()
			.cmp(&size_b.is_some())
			.then(name_a.cmp(name_b))
	});
	let items: Vec<_> = items
		.into_iter()
		.map(|(name, size)| {
			let href = PathBuf::from(relative_path)
				.join(&name)
				.display()
				.to_string();
			let trailing_slash = if size.is_some() { "" } else { "/" };
			let filename = format!("{name}{trailing_slash}");

			let link = format!("<span><a href=\"{href}\">{filename}</a>");
			let size = size.map(format_size).unwrap_or_default() + "</span>\n";
			// NOTE: emojis in filenames will probably cause misalignment
			let width = filename.chars().count();
			(link, size, width)
		})
		.collect();

	let name_width = items
		.iter()
		.map(|&(_, _, width)| width)
		.max()
		.unwrap_or_default();

	let mut list = String::new();
	for (link, filesize, width) in items {
		let spaces = " ".repeat(name_width - width + 1);
		let entry = format!("{link}{spaces}{filesize}");
		list.push_str(&entry);
	}
	let parent = if relative_path != "/" {
		"<span><a href=\"..\">../</a></span>\n"
	} else {
		""
	};
	let page = format!(
		r#"<!DOCTYPE html>
<head>
	<meta charset="UTF-8">
	<title>Index of {relative_path}</title>
	<style>
		html {{ color-scheme: dark; }}
		span:nth-child(odd) {{ background-color: #222; }}
		pre {{ font-size: 1.8em; }}
	</style>
</head>
<body>
	<h3>Index of {relative_path}</h3>
	<pre>
{parent}{list}	</pre>
</body>
</html>"#
	);
	Some(Content::html(page))
}

fn format_size(bytes: u64) -> String {
	if bytes < 1024 {
		format!("{bytes:>5}   B")
	} else if bytes < 1024 * 1024 {
		format!("{:>5.1} KiB", bytes as f64 / 1024.0)
	} else if bytes < 1024 * 1024 * 1024 {
		format!("{:>5.1} MiB", bytes as f64 / (1024.0 * 1024.0))
	} else {
		format!("{:>5.1} GiB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
	}
}

fn formatted_time_now() -> String {
	let unix_time = SystemTime::now()
		.duration_since(SystemTime::UNIX_EPOCH)
		.unwrap()
		.as_secs();

	let second = unix_time % 60;
	let minute = unix_time / 60 % 60;
	let hour = unix_time / 3600 % 24;

	let days_since_epoch = unix_time / (3600 * 24);
	let years_since_epoch = (days_since_epoch * 400) / 146097;
	// 365.2425 days per year
	/*
	days = years * 365 + years/4 + years/400 - years/100
	d = y*365 + y/4 + y/400 - y/100
	d = (365y*400)/400 + 100y/400 + y/400 - 4y/400
	d*400 = (365y*400) + 100y + y - 4y
	d*400 = 400*365*y + 97*y
	d*400 = y* (400*365 + 97)
	d*400 = y*146097
	years = (days * 400) / 146097
	*/
	let year = years_since_epoch + 1970;

	let is_leap_year = (year % 4 == 0) && !((year % 100 == 0) && !(year % 400 == 0));
	let feb = if is_leap_year { 29 } else { 28 };
	let month_lengths = [31, feb, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

	let leap_days = years_since_epoch / 4;
	let mut day = days_since_epoch - leap_days - years_since_epoch * 365;
	let mut month = 0;
	for i in 0..12 {
		if day < month_lengths[i] {
			month = i + 1;
			day = day + 1;
			break;
		}
		day -= month_lengths[i];
	}

	format!("{year}-{month:02}-{day:02}_{hour:02}:{minute:02}:{second:02}")
}

fn mime_type(ext: &str) -> Option<&'static str> {
	let t = match ext {
		"txt" | "md" | "toml" => "text/plain",
		"html" | "htm" => "text/html",
		"css" => "text/css",

		"apng" => "image/apng",
		"bmp" => "image/bmp",
		"gif" => "image/gif",
		"jpeg" | "jpg" => "image/jpeg",
		"png" => "image/png",
		"svg" => "image/svg+xml",
		"tif" | "tiff" => "image/tiff",
		"webp" => "image/webp",

		"aac" => "audio/aac",
		"mp3" => "audio/mpeg",
		"oga" | "ogg" => "audio/ogg",
		"opus" => "audio/opus",
		"wav" => "audio/wav",
		"weba" => "audio/webm",

		"3gp" => "video/3gpp",
		"3gp2" => "video/3gpp2",
		"avi" => "video/x-msvideo",
		"mov" => "video/mov",
		"mp4" => "video/mp4",
		"mpeg" => "video/mpeg",
		"ogv" => "video/ogv",
		"webm" => "video/webm",

		"json" => "application/json",
		_ => return None,
	};
	Some(t)
}
