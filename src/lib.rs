use std::{
    ffi::{CStr, CString, NulError},
    fmt::Display,
    fs::{self, File},
    io::{Read, Seek, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        raw::{c_char, c_void},
    },
    path::PathBuf,
    ptr::{null, null_mut},
};

use tempfile::{Builder, NamedTempFile, TempDir, tempfile};

include!("bindings.rs");

pub struct PgConn {
    pub conn: *mut PGconn,
}

pub struct PgResult {
    pub res: *mut PGresult,
}

impl Drop for PgConn {
    fn drop(&mut self) {
        unsafe {
            PQfinish(self.conn);
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
}

impl ToString for PgResult {
    fn to_string(&self) -> String {
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
        s
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn catch_notices() {
        let mut conn =
            PgConn::connect_db_env_vars().expect("Failed to create PGconn from connection string.");

        conn.trace("trace.log");

        let mut w = Vec::new();

        let _w_pusher = conn.set_notice_processor(|s| w.push(s));

        assert_eq!(conn.status(), ConnStatusType_CONNECTION_OK);

        let query = "do $$ begin raise notice 'Hello,'; raise notice 'world!'; end $$; select 1 as one, 2 as two;";

        let mut res = conn.exec(query).expect("Failed to execute query.");

        res.print("res.out", true, true, "|", true, false, false, false);

        let s = fs::read_to_string("res.out").expect("Should have been able to read the file");

        assert_eq!(res.to_string(), s);

        assert_eq!(res.status(), ExecStatusType_PGRES_TUPLES_OK);
        assert_eq!(res.error_message(), "");
        assert!(res.error_field(PG_DIAG_SEVERITY).is_none());
        assert_eq!(res.cmd_status(), "SELECT 1");

        assert_eq!(w.len(), 2);
        assert_eq!(w[0], "NOTICE:  Hello,\n");
        assert_eq!(w[1], "NOTICE:  world!\n");
    }

    #[test]
    fn lib_version() {
        unsafe {
            assert_eq!(PQlibVersion(), 180001);
        }
    }
}
