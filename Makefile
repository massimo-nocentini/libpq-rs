
build:
	cargo build --release

test:
# 	DATABASE_URL="postgres://db:db@host.docker.internal:5442/db" cargo test --release -- --nocapture
	DATABASE_URL="postgres://db:db@localhost:5442/db" cargo test --release --lib -- --nocapture
	
doc:
	cargo doc --document-private-items --release
	rm -rf docs && mv target/doc docs

bindgen:
	bindgen /opt/homebrew/opt/postgresql@18/include/postgresql/libpq-fe.h -o src/bindings.rs

docker-build:
	docker build -t ghcr.io/massimo-nocentini/libpq-rs:master .

docker-run:
	docker run -it --rm -e DATABASE_URL="postgres://db:db@host.docker.internal:5442/db" ghcr.io/massimo-nocentini/libpq-rs:master make test