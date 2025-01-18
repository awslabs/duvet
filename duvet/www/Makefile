public/script.js: node_modules $(wildcard src/*.js) rollup.config.mjs
	@NODE_ENV="production" npm run build

node_modules:
	@npm install

dev: node_modules
	@npm start

test:
	@node --test

.PHONY: dev test
