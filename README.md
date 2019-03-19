# mio_webserver
Non-Blocking I/O web server with rust/mio.

# How to use

```
$ cargo run [addr]:[port]
```

then connect via telnet, nc or browser


# Specification

* Return the response as HTTP 1.0.
* Accept only HTTP 1.0, 1.1.
* Only accept GET method.
* Disconnect the connection when the request and the corresponding response are exchanged.
* Security is not considered.（directory traversal etc）
