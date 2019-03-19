# mio_webserver
Non-Blocking I/O web server with rust/mio.

# How to use

```
$ cargo run [addr]:[port]
```

then connect via telnet, nc or browser

![sheep](https://user-images.githubusercontent.com/27873650/54591218-24c08c00-4a6d-11e9-9ead-49494b0adffc.png "sheep")

# Specification

* Return the response as HTTP 1.0.
* Accept only HTTP 1.0, 1.1.
* Only accept GET method.
* Disconnect the connection when the request and the corresponding response are exchanged.
* Security is not considered.（directory traversal etc）
