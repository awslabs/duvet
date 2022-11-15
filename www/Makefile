public/script.js: node_modules package-lock.json $(wildcard src/*.js)
	@rm -rf build
	@npm run build
	@awk 1 build/static/js/*.js > public/script.js

node_modules: package-lock.json
	@npm install

dev: node_modules src/result.test.json
	@npm start

.PHONY: dev
