#[derive(Debug)]
pub struct Request {
	pub method: Method,
	pub path: String,
	pub host: String,
	pub real_ip: Option<String>,
	pub range: Option<RequestRange>,
}

#[derive(Debug)]
pub enum RequestRange {
	From(usize),
	Full(usize, usize),
	Suffix(usize),
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Method {
	Get,
	Head,
}

pub struct Response {
	pub status: Status,
	pub content: Option<Content>,
}

#[derive(Debug, Clone, Copy)]
#[repr(u16)]
pub enum Status {
	Ok = 200,
	PartialContent = 206,
	BadRequest = 400,
	NotFound = 404,
}

#[derive(Debug, Clone)]
pub struct Content {
	mime_type: &'static str,
	range: Option<(usize, usize, usize)>,
	bytes: Vec<u8>,
}

impl Request {
	pub fn parse(source: &str) -> Option<Self> {
		let mut lines = source.lines();
		let head = lines.next()?;
		let (method, head) = head.split_once(' ')?;
		let method = Method::parse(method)?;
		let (path, version) = head.split_once(' ')?;
		_ = version.strip_prefix("HTTP/1")?;

		// parse http headers
		let mut host = None;
		let mut range = None;
		let mut real_ip = None;
		for line in lines {
			if line.is_empty() {
				break;
			}
			let line = line.to_lowercase();
			let (key, value) = line.split_once(": ")?;
			match key {
				"host" => host = Some(value.to_owned()),
				"range" => range = RequestRange::parse(value),
				"x-real-ip" => real_ip = Some(value.to_owned()),
				_ => (),
			}
		}
		let host = host?;

		// parse percent encoding
		let mut path_bytes = path.bytes();
		let mut path = Vec::with_capacity(path.len());
		while let Some(byte) = path_bytes.next() {
			if byte == b'%' {
				let s = String::from_utf8(vec![path_bytes.next()?, path_bytes.next()?]).ok()?;
				path.push(u8::from_str_radix(&s, 16).ok()?);
			} else {
				path.push(byte);
			}
		}
		let path = String::from_utf8(path).ok()?;

		Some(Self {
			method,
			path,
			host,
			real_ip,
			range,
		})
	}
}

impl Response {
	pub fn new(status: Status) -> Self {
		Self {
			status,
			content: None,
		}
	}

	pub fn format(mut self, head_only: bool) -> Vec<u8> {
		if let Some(content) = self.content {
			if content.range.is_some() {
				self.status = Status::PartialContent;
			}
			let mut buffer = format!(
				"{}\r\nContent-Type: {}\r\nContent-Length: {}\r\n",
				self.status.header(),
				content.mime_type,
				content.bytes.len(),
			)
			.into_bytes();
			if let Some((start, end, size)) = content.range {
				buffer.extend_from_slice(
					format!("Content-Range: bytes {start}-{end}/{size}\r\n").as_bytes(),
				);
			}
			buffer.extend_from_slice(b"\r\n");
			if !head_only {
				buffer.extend_from_slice(&content.bytes);
			}
			buffer
		} else {
			format!("{}\r\n\r\n", self.status.header()).into_bytes()
		}
	}

	pub fn with_content(mut self, content: Content) -> Self {
		self.content = Some(content);
		self
	}
}

impl Content {
	pub fn html(text: String) -> Self {
		Self::file("html", text.into_bytes())
	}

	pub fn text(text: String) -> Self {
		Self::file("txt", text.into_bytes())
	}

	pub fn file(ext: &str, bytes: Vec<u8>) -> Self {
		let content_type = match ext {
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
			"gz" => "application/gzip",
			_ => {
				if bytes.is_ascii() {
					"text/plain"
				} else {
					"application/octet-stream"
				}
			}
		};
		Self {
			mime_type: content_type,
			range: None,
			bytes,
		}
	}

	pub fn with_range(mut self, range: Option<(usize, usize, usize)>) -> Self {
		self.range = range;
		self
	}
}

impl Status {
	pub fn header(self) -> String {
		format!("HTTP/1.1 {} {}", self.code(), self.name())
	}

	pub fn code(self) -> u16 {
		self as u16
	}

	pub fn name(self) -> &'static str {
		match self {
			Status::Ok => "OK",
			Status::PartialContent => "PARTIAL CONTENT",
			Status::BadRequest => "BAD REQUEST",
			Status::NotFound => "NOT FOUND",
		}
	}
}

impl Method {
	fn parse(source: &str) -> Option<Self> {
		match source {
			"GET" => Some(Self::Get),
			"HEAD" => Some(Self::Head),
			_ => None,
		}
	}
}

impl std::fmt::Display for Method {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Method::Get => write!(f, "GET"),
			Method::Head => write!(f, "HEAD"),
		}
	}
}

impl RequestRange {
	fn parse(source: &str) -> Option<Self> {
		let source = source.strip_prefix("bytes=")?;
		let (start, end) = source.split_once('-')?;
		match (start.is_empty(), end.is_empty()) {
			(true, true) => None,
			(true, false) => Some(Self::Suffix(end.parse().ok()?)),
			(false, true) => Some(Self::From(start.parse().ok()?)),
			(false, false) => Some(Self::Full(start.parse().ok()?, end.parse().ok()?)),
		}
	}
}
