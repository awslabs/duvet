default: www/public/script.js
	@cargo test

publish: www/public/script.js
	@git diff --exit-code
	@cargo publish --allow-dirty

www/public/script.js:
	@$(MAKE) -C www

.PHONY: default publish force-build
