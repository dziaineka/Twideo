NAME   := skaborik/twideo_bot
TAG    := $$(git describe --tags --abbrev=0)
IMG    := ${NAME}:${TAG}

build:
	@docker build -t ${IMG} .

push:
	@docker push ${IMG}

up:
	@docker compose up --build

check:
	cargo fmt --check
	cargo clippy

format:
	cargo fmt
