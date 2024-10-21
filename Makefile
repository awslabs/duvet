default: duvet/www/public/script.js
	@cargo test

publish: duvet/www/public/script.js
	@git diff --exit-code
	@cargo publish --allow-dirty

duvet/www/public/script.js:
	@$(MAKE) -C duvet/www

changelog:
	@npx conventional-changelog-cli -p conventionalcommits -i CHANGELOG.md -s

.PHONY: default publish force-build changelog
