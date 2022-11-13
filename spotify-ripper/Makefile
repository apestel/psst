all: build

clean:
	rm -rf target/*
	rm dist/output.css

static:
	$(MAKE) -C frontend all
	rm -rf dist
	mkdir -p dist
	cp -R frontend/dist/* dist/

sqlx-setup:
	cargo install sqlx-cli
	sqlx database create
	sqlx migrate --source db/migrations run

db-migrate:
	sqlx migrate --source db/migrations run
	cargo sqlx prepare

build: css
	cargo sqlx prepare
	cargo build

push:
	docker build -t europe-west1-docker.pkg.dev/ops4ops-training-sandbox/default-docker-registry/spotify-ripper:0.0.15 .
	docker push europe-west1-docker.pkg.dev/ops4ops-training-sandbox/default-docker-registry/spotify-ripper:0.0.15
