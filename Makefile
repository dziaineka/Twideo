start_env:
	docker compose up -d --build

kill_env:
	docker compose kill

clean_env:
	( \
	set +e; \
	docker container prune -f; \
	docker volume prune -f; \
	docker network prune -f; \
	set -e; \
	)
