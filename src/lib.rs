use std::{
    ffi::NulError,
    os::raw::{c_char, c_void},
};

include!("bindings.rs");

impl PGconn {
    fn from_str(s: &str) -> Result<*mut Self, NulError> {
        unsafe {
            let conninfo = std::ffi::CString::new(s)?;
            Ok(PQconnectdb(conninfo.as_ptr()))
        }
    }

    fn status(&self) -> ConnStatusType {
        unsafe { PQstatus(self) }
    }

    fn exec(&self, query: &str) -> Result<*mut PGresult, NulError> {
        unsafe {
            let c_query = std::ffi::CString::new(query)?;
            Ok(PQexec(self as *const _ as *mut _, c_query.as_ptr()))
        }
    }

    fn set_notice_processor(
        &mut self,
        proc: Option<unsafe extern "C" fn(*mut std::os::raw::c_void, *const std::os::raw::c_char)>,
        arg: *mut std::os::raw::c_void,
    ) {
        unsafe {
            PQsetNoticeProcessor(self, proc, arg);
        }
    }
}

impl PGresult {
    fn status(&self) -> ExecStatusType {
        unsafe { PQresultStatus(self) }
    }

    fn cmd_status(&mut self) -> String {
        unsafe {
            let s = PQcmdStatus(self);
            std::ffi::CStr::from_ptr(s).to_string_lossy().into_owned()
        }
    }

    fn error_message(&self) -> String {
        unsafe {
            let s = PQresultErrorMessage(self);
            std::ffi::CStr::from_ptr(s).to_string_lossy().into_owned()
        }
    }

    fn error_field(&self, field_code: u8) -> String {
        unsafe {
            let s = PQresultErrorField(self, field_code.into());

            if s.is_null() {
                "".to_string()
            } else {
                std::ffi::CStr::from_ptr(s).to_string_lossy().into_owned()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::os::raw::{c_char, c_void};

    use super::*;

    extern "C" fn recv(data: *mut c_void, b: *const c_char) {
        unsafe {
            let s = std::ffi::CStr::from_ptr(b).to_string_lossy().into_owned();

            let notices: &mut Vec<String> = &mut *(data as *mut Vec<String>);

            notices.push(s);
        }
    }

    #[test]
    fn it_works() {
        unsafe {
            let conn_str = std::env::var("DATABASE_URL")
                .expect("Env var DATABASE_URL is required for this example.");

            let conn = PGconn::from_str(&conn_str)
                .expect("Failed to create PGconn from connection string.");

            let mut w = Vec::new();

            conn.as_mut()
                .unwrap()
                .set_notice_processor(Some(recv), &mut w as *mut Vec<String> as *mut c_void); //Vec::new().as_mut_ptr()

            assert_eq!(
                conn.as_ref().unwrap().status(),
                ConnStatusType_CONNECTION_OK
            );

            assert_eq!(PQstatus(conn), ConnStatusType_CONNECTION_OK);

            let query = "do $$ begin raise notice 'Hello, world!'; end $$; select 1;";

            let res = conn
                .as_ref()
                .unwrap()
                .exec(query)
                .expect("Failed to execute query.");

            assert_eq!(
                res.as_ref().unwrap().status(),
                ExecStatusType_PGRES_TUPLES_OK
            );

            assert_eq!(res.as_mut().unwrap().cmd_status(), "SELECT 1");

            assert_eq!(res.as_ref().unwrap().error_message(), "");

            assert_eq!(res.as_ref().unwrap().error_field(PG_DIAG_SEVERITY), "");

            assert_eq!(w.len(), 1);
            assert_eq!(w[0], "NOTICE:  Hello, world!\n");

            PQclear(res);
            PQfinish(conn);
        }
    }

    #[test]
    fn lib_version() {
        unsafe {
            assert_eq!(PQlibVersion(), 180001);
        }
    }
}
