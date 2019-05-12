#[derive(Fail, Debug)]
#[fail(display = "failed to parse http request")]
pub struct HttpParseError {
    status_code: u16
}
