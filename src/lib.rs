use std::{
    ffi::NulError,
    os::raw::{c_char, c_void},
};

include!("bindings.rs");

pub struct PgConn {
    conn: *mut PGconn,
}

pub struct PgResult {
    res: *mut PGresult,
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
    fn connectdb(s: &str) -> Result<PgConn, NulError> {
        unsafe {
            let conninfo = std::ffi::CString::new(s)?;
            let conn = PQconnectdb(conninfo.as_ptr());
            Ok(PgConn { conn })
        }
    }

    fn status(&self) -> ConnStatusType {
        unsafe { PQstatus(self.conn) }
    }

    fn exec(&self, query: &str) -> Result<PgResult, NulError> {
        unsafe {
            let c_query = std::ffi::CString::new(query)?;
            let res = PQexec(self.conn, c_query.as_ptr());
            Ok(PgResult { res })
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
            PQsetNoticeProcessor(self.conn, Some(Self::recv::<F>), a);
            b
        }
    }
}

impl PgResult {
    fn status(&self) -> ExecStatusType {
        unsafe { PQresultStatus(self.res) }
    }

    fn cmd_status(&mut self) -> String {
        unsafe {
            let s = PQcmdStatus(self.res);
            std::ffi::CStr::from_ptr(s).to_string_lossy().into_owned()
        }
    }

    fn error_message(&self) -> String {
        unsafe {
            let s = PQresultErrorMessage(self.res);
            std::ffi::CStr::from_ptr(s).to_string_lossy().into_owned()
        }
    }

    fn error_field(&self, field_code: u8) -> Option<String> {
        unsafe {
            let s = PQresultErrorField(self.res, field_code.into());
            if s.is_null() {
                None
            } else {
                Some(std::ffi::CStr::from_ptr(s).to_string_lossy().into_owned())
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn catch_notices() {
        let conn_str = std::env::var("DATABASE_URL")
            .expect("Env var DATABASE_URL is required for this example.");

        let mut conn =
            PgConn::connectdb(&conn_str).expect("Failed to create PGconn from connection string.");

        let mut w = Vec::new();

        let _w_pusher = conn.set_notice_processor(|s| w.push(s));

        assert_eq!(conn.status(), ConnStatusType_CONNECTION_OK);

        let query = "do $$ begin raise notice 'Hello,'; raise notice 'world!'; end $$; select 1;";

        let mut res = conn.exec(query).expect("Failed to execute query.");

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
