use std::{
    ffi::{CString, NulError},
    fmt::Display,
    io::{Read, Seek},
    ops::ControlFlow,
    os::raw::{c_char, c_void},
    ptr::null_mut,
};

use std::fmt::Debug;

use tempfile::Builder;

include!("bindings.rs");

pub struct PgSocket {
    socket: i32,
}

pub enum PgSocketPollResult {
    Timeout,
    Error(String),
}

impl Display for PgSocketPollResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PgSocketPollResult::Timeout => write!(f, "Timeout"),
            PgSocketPollResult::Error(s) => write!(f, "Error: {}", s),
        }
    }
}

impl PgSocket {
    pub fn poll(
        &self,
        read: bool,
        write: bool,
        timeout: Option<f64>,
    ) -> Result<(), PgSocketPollResult> {
        unsafe {
            let timeout_ms = match timeout {
                Some(t) => PQgetCurrentTimeUSec() + (t * 1000000.0) as i64,
                None => -1,
            };

            match PQsocketPoll(self.socket, read.into(), write.into(), timeout_ms) {
                a if a > 0 => Ok(()),
                0 => Err(PgSocketPollResult::Timeout),
                _ => Err(PgSocketPollResult::Error(
                    std::io::Error::last_os_error().to_string(),
                )),
            }
        }
    }
}
pub struct PgConn {
    conn: *mut PGconn,
}

unsafe impl Send for PgConn {}

unsafe impl Sync for PgConn {}

pub struct PgResult {
    res: *mut PGresult,
}

pub struct PgNotify {
    notify: *mut PGnotify,
}

impl PgNotify {
    pub fn relname(&self) -> String {
        unsafe {
            let s = (*self.notify).relname;
            std::ffi::CStr::from_ptr(s).to_string_lossy().into_owned()
        }
    }

    pub fn be_pid(&self) -> i32 {
        unsafe { (*self.notify).be_pid }
    }

    pub fn extra(&self) -> String {
        unsafe {
            let s = (*self.notify).extra;
            std::ffi::CStr::from_ptr(s).to_string_lossy().into_owned()
        }
    }
}

impl Drop for PgConn {
    fn drop(&mut self) {
        unsafe {
            PQfinish(self.conn);
        }
    }
}

impl Drop for PgNotify {
    fn drop(&mut self) {
        unsafe {
            PQfreemem(self.notify as *mut c_void);
        }
    }
}

impl Drop for PgResult {
    fn drop(&mut self) {
        unsafe {
            PQclear(self.res);
        }
    }
}

impl PgConn {
    /// Connect to the database using environment variables.
    ///
    /// See the [official doc](https://www.postgresql.org/docs/current/libpq-envars.html).
    pub fn connect_db_env_vars() -> Result<PgConn, NulError> {
        Self::connect_db("")
    }

    pub fn connect_db(s: &str) -> Result<PgConn, NulError> {
        unsafe {
            let conninfo = std::ffi::CString::new(s)?;
            let conn = PQconnectdb(conninfo.as_ptr());
            Ok(PgConn { conn })
        }
    }

    pub fn status(&self) -> ConnStatusType {
        unsafe { PQstatus(self.conn) }
    }

    pub fn exec(&self, query: &str) -> Result<PgResult, NulError> {
        unsafe {
            let c_query = std::ffi::CString::new(query)?;
            let res = PQexec(self.conn, c_query.as_ptr());
            Ok(PgResult { res })
        }
    }

    pub fn exec_file(&self, file_path: &str) -> Result<PgResult, NulError> {
        let content = std::fs::read_to_string(file_path).expect("Failed to read file.");
        self.exec(&content)
    }

    pub fn trace(&mut self, file: &str) {
        unsafe {
            let c_file = std::ffi::CString::new(file).unwrap();
            let mode = std::ffi::CString::new("w").unwrap();
            let fp = fopen(c_file.as_ptr(), mode.as_ptr());
            PQtrace(self.conn, fp);
            assert_eq!(fflush(fp), 0);
        }
    }

    pub fn untrace(&mut self) {
        unsafe {
            PQuntrace(self.conn);
        }
    }

    pub fn socket(&self) -> PgSocket {
        unsafe {
            PgSocket {
                socket: PQsocket(self.conn),
            }
        }
    }

    pub fn consume_input(&mut self) -> Result<(), String> {
        unsafe {
            if PQconsumeInput(self.conn) == 0 {
                Err(self.error_message())
            } else {
                Ok(())
            }
        }
    }

    pub fn notifies(&mut self) -> Option<PgNotify> {
        unsafe {
            let notify = PQnotifies(self.conn);
            if notify.is_null() {
                None
            } else {
                Some(PgNotify { notify })
            }
        }
    }

    pub fn error_message(&self) -> String {
        unsafe {
            let s = PQerrorMessage(self.conn);
            if s.is_null() {
                "".to_string()
            } else {
                std::ffi::CStr::from_ptr(s).to_string_lossy().into_owned()
            }
        }
    }

    pub fn notify(&mut self, channel: &str, payload: Option<&str>) -> Result<PgResult, NulError> {
        let query = match payload {
            Some(p) => format!("NOTIFY {}, '{}';", channel, p),
            None => format!("NOTIFY {};", channel),
        };
        self.exec(&query)
    }

    pub fn listen(&mut self, channel: &str) -> Result<PgResult, NulError> {
        let query = format!("LISTEN {};", channel);
        self.exec(&query)
    }

    ///
    /// A callback function to receive notices from the server.
    /// https://stackoverflow.com/questions/24191249/working-with-c-void-in-an-ffi
    /// https://adventures.michaelfbryan.com/posts/rust-closures-in-ffi/
    extern "C" fn ffi_notice_processor<F>(arg: *mut c_void, data: *const c_char)
    where
        F: FnMut(String),
    {
        unsafe {
            let s = std::ffi::CStr::from_ptr(data)
                .to_string_lossy()
                .into_owned();

            let f = &mut *(arg as *mut F);

            f(s);
        }
    }

    pub fn set_notice_processor<F>(&mut self, proc: F) -> Box<F>
    where
        F: FnMut(String),
    {
        unsafe {
            let mut b = Box::new(proc);
            let a = b.as_mut() as *mut F as *mut c_void;
            PQsetNoticeProcessor(self.conn, Some(Self::ffi_notice_processor::<F>), a);
            b
        }
    }

    extern "C" fn ffi_notice_receiver<F>(arg: *mut c_void, data: *const PGresult)
    where
        F: FnMut(PgResult),
    {
        unsafe {
            let s = PgResult {
                res: data as *mut PGresult,
            };

            let f = &mut *(arg as *mut F);

            f(s);
        }
    }

    /// Sets a notice receiver function to receive notices from the server.
    /// Notices are sent to the receiver after command execution is completed.
    /// https://www.postgresql.org/docs/current/libpq-notice-processing.html
    pub fn set_notice_receiver<F>(&mut self, proc: F) -> Box<F>
    where
        F: FnMut(PgResult),
    {
        unsafe {
            let mut b = Box::new(proc);
            let a = b.as_mut() as *mut F as *mut c_void;
            PQsetNoticeReceiver(self.conn, Some(Self::ffi_notice_receiver::<F>), a);
            b
        }
    }

    pub fn listen_loop<F, T>(&mut self, timeout_sec: Option<f64>, proc: F) -> Vec<T>
    where
        F: Fn(usize, PgNotify) -> ControlFlow<(), Option<T>>,
    {
        let mut recvs = Vec::new();

        let mut count = 0;

        loop {
            match self.socket().poll(true, false, timeout_sec) {
                Ok(()) => {
                    self.consume_input().expect("Failed to consume input.");

                    while let Some(notify) = self.notifies() {
                        match proc(count, notify) {
                            ControlFlow::Continue(Some(p)) => recvs.push(p),
                            ControlFlow::Break(()) => {
                                break;
                            }
                            _ => {} // Do nothing
                        }
                        self.consume_input().expect("Failed to consume input.");
                        count += 1;
                    }
                }
                Err(_e) => break,
            }
        }

        recvs
    }
}

impl PgResult {
    pub fn status(&self) -> ExecStatusType {
        unsafe { PQresultStatus(self.res) }
    }

    pub fn cmd_status(&mut self) -> String {
        unsafe {
            let s = PQcmdStatus(self.res);
            std::ffi::CStr::from_ptr(s).to_string_lossy().into_owned()
        }
    }

    pub fn error_message(&self) -> String {
        unsafe {
            let s = PQresultErrorMessage(self.res);
            std::ffi::CStr::from_ptr(s).to_string_lossy().into_owned()
        }
    }

    pub fn error_field(&self, field_code: u8) -> Option<String> {
        unsafe {
            let s = PQresultErrorField(self.res, field_code.into());
            if s.is_null() {
                None
            } else {
                Some(std::ffi::CStr::from_ptr(s).to_string_lossy().into_owned())
            }
        }
    }

    pub fn get_value<T>(&self, row: i32, col: i32) -> Option<T>
    where
        T: std::str::FromStr,
    {
        unsafe {
            let s = PQgetvalue(self.res, row, col);
            if s.is_null() {
                None
            } else {
                let s = std::ffi::CStr::from_ptr(s).to_string_lossy().into_owned();
                match s.parse::<T>() {
                    Ok(v) => Some(v),
                    Err(_) => None,
                }
            }
        }
    }

    /// Print the result to a file.
    /// See the [official doc](https://www.postgresql.org/docs/current/libpq-exec.html#LIBPQ-PQPRINT
    pub fn print(
        &self,
        filename: &str,
        header: bool,
        align: bool,
        fieldsep: &str,
        standard: bool,
        html3: bool,
        expanded: bool,
        pager: bool,
    ) {
        unsafe {
            let sep = CString::new(fieldsep).unwrap();

            let printopt = PQprintOpt {
                header: header.into(),
                align: align.into(),
                fieldSep: sep.as_ptr() as *mut c_char,
                tableOpt: null_mut(),
                caption: null_mut(),
                standard: standard.into(),
                html3: html3.into(),
                expanded: expanded.into(),
                pager: pager.into(),
                fieldName: null_mut(),
            };

            let fp = fopen(
                CString::new(filename).unwrap().as_ptr(),
                CString::new("w").unwrap().as_ptr(),
            );

            PQprint(fp, self.res, &printopt);

            assert_eq!(fflush(fp), 0);
            assert_eq!(fclose(fp), 0);
        }
    }

    /// Get the value at the specified row and column.
    /// See also [PQgetvalue](https://www.postgresql.org/docs/current/libpq-exec.html#LIBPQ-PQGETVALUE).
    pub fn get_value_raw(&self, row: i32, col: i32) -> String {
        unsafe {
            let s = PQgetvalue(self.res, row, col);
            if s.is_null() {
                "".to_string()
            } else {
                std::ffi::CStr::from_ptr(s).to_string_lossy().into_owned()
            }
        }
    }
}

impl Display for PgResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut temp_file = Builder::new()
            .prefix("pg-res-")
            .suffix(".json")
            .tempfile()
            .unwrap();

        let temp_path = temp_file.path().to_path_buf();

        self.print(
            temp_path.as_path().to_str().unwrap(),
            true,
            true,
            "|",
            true,
            false,
            false,
            false,
        );

        let mut s = String::new();
        temp_file
            .seek(std::io::SeekFrom::Start(0))
            .expect("Failed to seek to start of temp file.");
        temp_file
            .read_to_string(&mut s)
            .expect("Failed to read temp file.");

        write!(f, "{}", s)
    }
}
