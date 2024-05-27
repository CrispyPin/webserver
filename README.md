# webserver
Simple http server written from scratch with no library depencencies.

## features
- list directory contents
- serve `index.html` for a directory that has such a file
- serve `/path/to/<something>.html` when you request `/path/to/<something>`, making urls a bit nicer
- partial file requests so you can watch large video files without needing to load all of it first
- logs requests to stdout

## usage
```
webserver [127.0.0.1:12345] [/path/to/site/root]
```
Both arguments are optional but you must specify the ip if you specify the path.

The default address is `127.0.0.1:55566` and it will serve the current directory.


