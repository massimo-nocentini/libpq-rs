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

    extern "C" fn recv<F>(arg: *mut c_void, data: *const c_char)
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

    fn set_notice_processor<F>(&mut self, proc: F) -> Box<F>
    where
        F: FnMut(String),
    {
        unsafe {
            let mut b = Box::new(proc);
            let a = b.as_mut() as *mut F as *mut c_void;
            PQsetNoticeProcessor(self, Some(Self::recv::<F>), a);
            b
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

    use super::*;

    #[test]
    fn it_works() {
        unsafe {
            let conn_str = std::env::var("DATABASE_URL")
                .expect("Env var DATABASE_URL is required for this example.");

            let conn = PGconn::from_str(&conn_str)
                .expect("Failed to create PGconn from connection string.");

            let conn_ref = conn.as_ref().unwrap();
            let conn_ref_mut = conn.as_mut().unwrap();

            let mut w = Vec::new();

            let _f = conn_ref_mut.set_notice_processor(|s| w.push(s));

            assert_eq!(conn_ref.status(), ConnStatusType_CONNECTION_OK);

            let query =
                "do $$ begin raise notice 'Hello,'; raise notice 'world!'; end $$; select 1;";

            let res = conn_ref.exec(query).expect("Failed to execute query.");
            let res_ref = res.as_ref().unwrap();
            let res_ref_mut = res.as_mut().unwrap();

            assert_eq!(res_ref.status(), ExecStatusType_PGRES_TUPLES_OK);
            assert_eq!(res_ref.error_message(), "");
            assert_eq!(res_ref.error_field(PG_DIAG_SEVERITY), "");
            assert_eq!(res_ref_mut.cmd_status(), "SELECT 1");

            assert_eq!(w.len(), 2);
            assert_eq!(w[0], "NOTICE:  Hello,\n");
            assert_eq!(w[1], "NOTICE:  world!\n");

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
