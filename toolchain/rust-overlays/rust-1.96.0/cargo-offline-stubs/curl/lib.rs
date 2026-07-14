use std::fmt;

#[derive(Clone, Debug)]
pub struct Error(u32);

impl Error {
    pub fn new(code: u32) -> Self {
        Self(code)
    }
    pub fn code(&self) -> u32 {
        self.0
    }
    pub fn description(&self) -> &str {
        "Ristux offline transport unavailable"
    }
    pub fn extra_description(&self) -> Option<&str> {
        None
    }
    pub fn set_extra(&mut self, _: String) {}
    pub fn is_aborted_by_callback(&self) -> bool {
        false
    }
    pub fn is_file_not_found(&self) -> bool {
        false
    }
    pub fn is_http2_stream_error(&self) -> bool {
        false
    }
    pub fn is_operation_timedout(&self) -> bool {
        false
    }
    pub fn is_couldnt_connect(&self) -> bool {
        false
    }
    pub fn is_couldnt_resolve_proxy(&self) -> bool {
        false
    }
    pub fn is_couldnt_resolve_host(&self) -> bool {
        false
    }
    pub fn is_recv_error(&self) -> bool {
        false
    }
    pub fn is_send_error(&self) -> bool {
        false
    }
    pub fn is_http2_error(&self) -> bool {
        false
    }
    pub fn is_ssl_connect_error(&self) -> bool {
        false
    }
    pub fn is_partial_file(&self) -> bool {
        false
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.description())
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Debug)]
pub struct MultiError;
impl MultiError {
    pub fn is_call_perform(&self) -> bool {
        false
    }
}
impl fmt::Display for MultiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Ristux offline transport unavailable")
    }
}
impl std::error::Error for MultiError {}

#[derive(Debug)]
pub struct Version;
impl Version {
    pub fn get() -> Self {
        Self
    }
    pub fn version(&self) -> &str {
        "ristux-offline"
    }
    pub fn vendored(&self) -> bool {
        false
    }
    pub fn ssl_version(&self) -> Option<&str> {
        None
    }
}

pub mod easy {
    use crate::Error;

    #[derive(Debug)]
    pub struct Easy;
    pub struct Easy2<T>(pub T);
    pub struct List;
    #[derive(Clone)]
    pub struct SslOpt;
    #[derive(Clone, Copy)]
    pub enum HttpVersion {
        V11,
        V2,
    }
    #[derive(Clone, Copy, Debug)]
    pub enum InfoType {
        Text,
        HeaderIn,
        HeaderOut,
        DataIn,
        DataOut,
        SslDataIn,
        SslDataOut,
    }
    #[derive(Clone, Copy)]
    pub enum SslVersion {
        Default,
        Tlsv1,
        Sslv2,
        Sslv3,
        Tlsv10,
        Tlsv11,
        Tlsv12,
        Tlsv13,
    }
    #[derive(Debug)]
    pub struct ReadError;
    #[derive(Debug)]
    pub struct WriteError;
    pub struct Transfer<'a>(std::marker::PhantomData<&'a mut Easy>);

    impl Easy {
        pub fn new() -> Self {
            Self
        }
        pub fn reset(&mut self) {}
        pub fn put(&mut self, _: bool) -> Result<(), Error> {
            Ok(())
        }
        pub fn get(&mut self, _: bool) -> Result<(), Error> {
            Ok(())
        }
        pub fn upload(&mut self, _: bool) -> Result<(), Error> {
            Ok(())
        }
        pub fn url(&mut self, _: &str) -> Result<(), Error> {
            Ok(())
        }
        pub fn custom_request(&mut self, _: &str) -> Result<(), Error> {
            Ok(())
        }
        pub fn in_filesize(&mut self, _: u64) -> Result<(), Error> {
            Ok(())
        }
        pub fn http_headers(&mut self, _: List) -> Result<(), Error> {
            Ok(())
        }
        pub fn response_code(&self) -> Result<u32, Error> {
            Ok(0)
        }
        pub fn effective_url(&self) -> Result<Option<&str>, Error> {
            Ok(None)
        }
        pub fn primary_ip(&self) -> Result<Option<&str>, Error> {
            Ok(None)
        }
        pub fn transfer(&mut self) -> Transfer<'_> {
            Transfer(std::marker::PhantomData)
        }
        pub fn follow_location(&mut self, _: bool) -> Result<(), Error> {
            Ok(())
        }
        pub fn http_version(&mut self, _: HttpVersion) -> Result<(), Error> {
            Ok(())
        }
        pub fn pipewait(&mut self, _: bool) -> Result<(), Error> {
            Ok(())
        }
        pub fn progress(&mut self, _: bool) -> Result<(), Error> {
            Ok(())
        }
        pub fn proxy(&mut self, _: &str) -> Result<(), Error> {
            Ok(())
        }
        pub fn cainfo(&mut self, _: &std::path::Path) -> Result<(), Error> {
            Ok(())
        }
        pub fn proxy_cainfo(&mut self, _: &str) -> Result<(), Error> {
            Ok(())
        }
        pub fn ssl_options(&mut self, _: &SslOpt) -> Result<(), Error> {
            Ok(())
        }
        pub fn useragent(&mut self, _: &str) -> Result<(), Error> {
            Ok(())
        }
        pub fn accept_encoding(&mut self, _: &str) -> Result<(), Error> {
            Ok(())
        }
        pub fn ssl_version(&mut self, _: SslVersion) -> Result<(), Error> {
            Ok(())
        }
        pub fn ssl_min_max_version(&mut self, _: SslVersion, _: SslVersion) -> Result<(), Error> {
            Ok(())
        }
        pub fn verbose(&mut self, _: bool) -> Result<(), Error> {
            Ok(())
        }
        pub fn connect_timeout(&mut self, _: std::time::Duration) -> Result<(), Error> {
            Ok(())
        }
        pub fn low_speed_time(&mut self, _: std::time::Duration) -> Result<(), Error> {
            Ok(())
        }
        pub fn low_speed_limit(&mut self, _: u32) -> Result<(), Error> {
            Ok(())
        }
        pub fn write_function<F>(&mut self, _: F) -> Result<(), Error>
        where
            F: FnMut(&[u8]) -> Result<usize, WriteError> + Send + 'static,
        {
            Ok(())
        }
        pub fn header_function<F>(&mut self, _: F) -> Result<(), Error>
        where
            F: FnMut(&[u8]) -> bool + Send + 'static,
        {
            Ok(())
        }
        pub fn progress_function<F>(&mut self, _: F) -> Result<(), Error>
        where
            F: FnMut(f64, f64, f64, f64) -> bool + Send + 'static,
        {
            Ok(())
        }
        pub fn debug_function<F>(&mut self, _: F) -> Result<(), Error>
        where
            F: FnMut(InfoType, &[u8]) + Send + 'static,
        {
            Ok(())
        }
    }
    impl List {
        pub fn new() -> Self {
            Self
        }
        pub fn append(&mut self, _: &str) -> Result<(), Error> {
            Ok(())
        }
    }
    impl Transfer<'_> {
        pub fn read_function<F>(&mut self, _: F) -> Result<(), Error>
        where
            F: FnMut(&mut [u8]) -> Result<usize, ReadError>,
        {
            Ok(())
        }
        pub fn write_function<F>(&mut self, _: F) -> Result<(), Error>
        where
            F: FnMut(&[u8]) -> Result<usize, WriteError>,
        {
            Ok(())
        }
        pub fn header_function<F>(&mut self, _: F) -> Result<(), Error>
        where
            F: FnMut(&[u8]) -> bool,
        {
            Ok(())
        }
        pub fn perform(&mut self) -> Result<(), Error> {
            Err(Error::new(1))
        }
    }
    pub trait Handler {
        fn write(&mut self, _: &[u8]) -> Result<usize, WriteError> {
            Ok(0)
        }
        fn header(&mut self, _: &[u8]) -> bool {
            true
        }
        fn read(&mut self, _: &mut [u8]) -> Result<usize, ReadError> {
            Ok(0)
        }
        fn debug(&mut self, _: InfoType, _: &[u8]) {}
        fn progress(&mut self, _: f64, _: f64, _: f64, _: f64) -> bool {
            true
        }
    }
    impl<T> Easy2<T> {
        pub fn new(value: T) -> Self {
            Self(value)
        }
        pub fn get_mut(&mut self) -> &mut T {
            &mut self.0
        }
        pub fn response_code(&self) -> Result<u32, Error> {
            Ok(0)
        }
        pub fn primary_ip(&self) -> Result<Option<&str>, Error> {
            Ok(None)
        }
        pub fn url(&mut self, _: &str) -> Result<(), Error> {
            Ok(())
        }
        pub fn follow_location(&mut self, _: bool) -> Result<(), Error> {
            Ok(())
        }
        pub fn nobody(&mut self, _: bool) -> Result<(), Error> {
            Ok(())
        }
        pub fn get(&mut self, _: bool) -> Result<(), Error> {
            Ok(())
        }
        pub fn post(&mut self, _: bool) -> Result<(), Error> {
            Ok(())
        }
        pub fn put(&mut self, _: bool) -> Result<(), Error> {
            Ok(())
        }
        pub fn upload(&mut self, _: bool) -> Result<(), Error> {
            Ok(())
        }
        pub fn post_field_size(&mut self, _: u64) -> Result<(), Error> {
            Ok(())
        }
        pub fn in_filesize(&mut self, _: u64) -> Result<(), Error> {
            Ok(())
        }
        pub fn custom_request(&mut self, _: &str) -> Result<(), Error> {
            Ok(())
        }
        pub fn http_headers(&mut self, _: List) -> Result<(), Error> {
            Ok(())
        }
        pub fn proxy(&mut self, _: &str) -> Result<(), Error> {
            Ok(())
        }
        pub fn cainfo(&mut self, _: &std::path::Path) -> Result<(), Error> {
            Ok(())
        }
        pub fn proxy_cainfo(&mut self, _: &str) -> Result<(), Error> {
            Ok(())
        }
        pub fn ssl_options(&mut self, _: &SslOpt) -> Result<(), Error> {
            Ok(())
        }
        pub fn useragent(&mut self, _: &str) -> Result<(), Error> {
            Ok(())
        }
        pub fn accept_encoding(&mut self, _: &str) -> Result<(), Error> {
            Ok(())
        }
        pub fn ssl_version(&mut self, _: SslVersion) -> Result<(), Error> {
            Ok(())
        }
        pub fn ssl_min_max_version(&mut self, _: SslVersion, _: SslVersion) -> Result<(), Error> {
            Ok(())
        }
        pub fn verbose(&mut self, _: bool) -> Result<(), Error> {
            Ok(())
        }
        pub fn http_version(&mut self, _: HttpVersion) -> Result<(), Error> {
            Ok(())
        }
        pub fn pipewait(&mut self, _: bool) -> Result<(), Error> {
            Ok(())
        }
        pub fn connect_timeout(&mut self, _: std::time::Duration) -> Result<(), Error> {
            Ok(())
        }
        pub fn low_speed_time(&mut self, _: std::time::Duration) -> Result<(), Error> {
            Ok(())
        }
        pub fn low_speed_limit(&mut self, _: u32) -> Result<(), Error> {
            Ok(())
        }
    }
    impl SslOpt {
        pub fn new() -> Self {
            Self
        }
        pub fn no_revoke(&mut self, _: bool) -> &mut Self {
            self
        }
    }
}

pub mod multi {
    use crate::easy::{Easy, Easy2};
    use crate::{Error, MultiError};
    use std::time::Duration;

    pub struct Multi;
    pub struct EasyHandle {
        easy: Easy,
        token: usize,
    }
    pub struct Easy2Handle<T> {
        easy: Easy2<T>,
        token: usize,
    }
    pub struct Message;
    pub struct WaitFd;

    impl Multi {
        pub fn new() -> Self {
            Self
        }
        pub fn pipelining(&mut self, _: bool, _: bool) -> Result<(), MultiError> {
            Ok(())
        }
        pub fn set_max_host_connections(&mut self, _: usize) -> Result<(), MultiError> {
            Ok(())
        }
        pub fn add(&self, easy: Easy) -> Result<EasyHandle, MultiError> {
            Ok(EasyHandle { easy, token: 0 })
        }
        pub fn add2<T>(&self, easy: Easy2<T>) -> Result<Easy2Handle<T>, MultiError> {
            Ok(Easy2Handle { easy, token: 0 })
        }
        pub fn remove(&self, handle: EasyHandle) -> Result<Easy, MultiError> {
            Ok(handle.easy)
        }
        pub fn remove2<T>(&self, handle: Easy2Handle<T>) -> Result<Easy2<T>, MultiError> {
            Ok(handle.easy)
        }
        pub fn perform(&self) -> Result<u32, MultiError> {
            Ok(0)
        }
        pub fn messages<F>(&self, _: F)
        where
            F: FnMut(Message),
        {
        }
        pub fn get_timeout(&self) -> Result<Option<Duration>, MultiError> {
            Ok(None)
        }
        pub fn wait(&self, _: &mut [WaitFd], _: Duration) -> Result<(), MultiError> {
            Ok(())
        }
    }
    impl EasyHandle {
        pub fn set_token(&mut self, token: usize) -> Result<(), Error> {
            self.token = token;
            Ok(())
        }
        pub fn response_code(&self) -> Result<u32, Error> {
            self.easy.response_code()
        }
    }
    impl<T> Easy2Handle<T> {
        pub fn set_token(&mut self, token: usize) -> Result<(), Error> {
            self.token = token;
            Ok(())
        }
    }
    impl Message {
        pub fn token(&self) -> Result<usize, MultiError> {
            Err(MultiError)
        }
        pub fn result_for(&self, _: &EasyHandle) -> Option<Result<(), Error>> {
            None
        }
        pub fn result_for2<T>(&self, _: &Easy2Handle<T>) -> Option<Result<(), Error>> {
            None
        }
    }
}
