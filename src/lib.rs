use std::{ffi::NulError};

include!("bindings.rs");

impl Drop for PGconn {
    fn drop(&mut self) {
        unsafe {
            PQfinish(self);
        }
    }
}

impl PGconn {
    fn from_str(s: &str) -> Result<*mut Self, NulError> {
        unsafe {
            let conninfo = std::ffi::CString::new(s)?;
            Ok(PQconnectdb(conninfo.as_ptr()))
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
            
            let conn = PGconn::from_str(&conn_str).expect("Failed to create PGconn from connection string.");
            assert_eq!(PQstatus(conn), ConnStatusType_CONNECTION_OK);
        }
    }
}
