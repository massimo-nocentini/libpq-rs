use std::{fs, thread};

use libpq::{
    ConnStatusType_CONNECTION_OK, ExecStatusType_PGRES_COMMAND_OK, ExecStatusType_PGRES_TUPLES_OK,
    PG_DIAG_SEVERITY, PQlibVersion, PgConn,
};

#[test]
fn catch_notices() {
    let mut conn =
        PgConn::connect_db_env_vars().expect("Failed to create PGconn from connection string.");

    assert_eq!(conn.status(), ConnStatusType_CONNECTION_OK);

    conn.trace("./test-out/trace.log");

    let mut w = Vec::new();

    let _w_pusher = conn.set_notice_processor(|s| w.push(s));

    let query = "do $$ begin raise notice 'Hello,'; raise notice 'world!'; end $$; select 1 as one, 2 as two;";

    let mut res = conn.exec(query).expect("Failed to execute query.");

    res.print(
        "./test-out/res.out",
        true,
        true,
        "|",
        true,
        false,
        false,
        false,
    );

    let s =
        fs::read_to_string("./test-out/res.out").expect("Should have been able to read the file");

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

/// Test for LISTEN/NOTIFY functionality.
///
/// https://www.postgresql.org/docs/current/libpq-example.html#LIBPQ-EXAMPLE-2
#[test]
fn listen_notify() {
    let handle = thread::spawn(|| {
        let mut conn =
            PgConn::connect_db_env_vars().expect("Failed to create PGconn from connection string.");

        assert_eq!(conn.status(), ConnStatusType_CONNECTION_OK);

        {
            let res = conn.exec("LISTEN TBL2").expect("Failed to execute LISTEN.");
            assert_eq!(res.status(), ExecStatusType_PGRES_COMMAND_OK);
        }

        let mut recvs = Vec::new();

        for _ in 0..5 {
            match conn.socket().poll(true, false, Some(10.0)) {
                Ok(()) => {
                    conn.consume_input().expect("Failed to consume input.");

                    while let Some(notify) = conn.notifies() {
                        assert_eq!(notify.relname(), "tbl2");
                        assert_eq!(notify.extra(), "");

                        recvs.push(notify.relname());

                        conn.consume_input().expect("Failed to consume input.");
                    }
                }
                Err(_e) => break,
            }
        }

        recvs
    });

    // Give the listener a moment to set up.
    thread::sleep(std::time::Duration::from_millis(100));

    // Now send some NOTIFY messages.

    let conn =
        PgConn::connect_db_env_vars().expect("Failed to create PGconn from connection string.");

    assert_eq!(conn.status(), ConnStatusType_CONNECTION_OK);

    for _ in 0..5 {
        let res = conn.exec("NOTIFY TBL2").expect("Failed to execute NOTIFY.");
        assert_eq!(res.status(), ExecStatusType_PGRES_COMMAND_OK);
    }

    let recvs = handle.join().expect("Thread panicked.");

    assert_eq!(recvs.len(), 5);
    assert_eq!(recvs, vec!["tbl2", "tbl2", "tbl2", "tbl2", "tbl2"]);
}

#[test]
fn listen_notify_api() {
    let handle = thread::spawn(|| {
        let mut conn =
            PgConn::connect_db_env_vars().expect("Failed to create PGconn from connection string.");

        assert_eq!(conn.status(), ConnStatusType_CONNECTION_OK);

        {
            let res = conn.exec("LISTEN TBL3").expect("Failed to execute LISTEN.");
            assert_eq!(res.status(), ExecStatusType_PGRES_COMMAND_OK);
        }

        conn.listen(Some(1.0), |notify| notify.relname())
    });

    // Give the listener a moment to set up.
    thread::sleep(std::time::Duration::from_millis(100));

    // Now send some NOTIFY messages.

    let conn =
        PgConn::connect_db_env_vars().expect("Failed to create PGconn from connection string.");

    assert_eq!(conn.status(), ConnStatusType_CONNECTION_OK);

    for _ in 0..5 {
        let res = conn.exec("NOTIFY TBL3").expect("Failed to execute NOTIFY.");
        assert_eq!(res.status(), ExecStatusType_PGRES_COMMAND_OK);
    }

    let recvs = handle.join().expect("Thread panicked.");

    assert_eq!(recvs.len(), 5);
    assert_eq!(recvs, vec!["tbl3", "tbl3", "tbl3", "tbl3", "tbl3"]);
}
