
build:
	cargo build --release

test:
# 	DATABASE_URL="postgres://db:db@host.docker.internal:5442/db" cargo test --release -- --nocapture
	RUST_BACKTRACE=1 PGHOST=localhost PGPORT=5442 PGDATABASE=db PGUSER=db PGPASSWORD=db cargo test --release --lib -- --nocapture
	
test-integration:
	RUST_BACKTRACE=1 cargo test --release --test integration_test -- --nocapture

doc:
	cargo doc --document-private-items --release
	rm -rf docs && mv target/doc docs

bindgen:
	bindgen /opt/homebrew/opt/postgresql@18/include/postgresql/libpq-fe.h -o src/bindings.rs

docker-build:
	docker build -t ghcr.io/massimo-nocentini/libpq-rs:master .

docker-run:
# 	docker run -it --rm -e DATABASE_URL="postgres://db:db@host.docker.internal:5442/db" ghcr.io/massimo-nocentini/libpq-rs:master make test
	docker run -it --rm -v ./test-out:/usr/src/libpq-rs/test-out -e PGHOST=host.docker.internal -e PGPORT=5442 -e PGDATABASE=db -e PGUSER=db -e PGPASSWORD=db ghcr.io/massimo-nocentini/libpq-rs:master make test-integration