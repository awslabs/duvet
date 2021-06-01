build:
	@npm install
	@npm run build
	@awk 1 build/static/js/*.js > public/script.js

dev: src/result.test.json
	@npm start

.PHONY: build dev
