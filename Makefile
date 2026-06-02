.PHONY: dev dev-backend dev-frontend check fmt fix build clean

# Run backend and frontend together
dev:
	make -j2 dev-backend dev-frontend

# Run the API backend
dev-backend:
	cargo run -p api

# Run the React frontend dev server
dev-frontend:
	cd frontend && npm run dev

# Build everything (release)
build:
	cargo build --release
	cd frontend && npm run build

# Format all code
fmt:
	cargo fmt --all
	cd frontend && npx eslint . --fix

# Check formatting, lints, and types without changing anything
check:
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings
	cd frontend && npx tsc --noEmit
	cd frontend && npx eslint .

# Clean build artifacts
clean:
	cargo clean
	rm -rf frontend/dist
