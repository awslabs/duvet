## 0.4.0 (2025-01-22)

### Features

* Added support for configuration files in place of command line arguments ([#152](https://github.com/awslabs/duvet/pull/152))
* New `snapshot` report output, which prevents accidental changes in requirement coverage ([#153](https://github.com/awslabs/duvet/pull/153))
* New `duvet init` command, which creates a configuration file based on the current directory ([#154](https://github.com/awslabs/duvet/pull/154))
* Detailed errors with specific line numbers about what went wrong.

### Bug Fixes

* More robust specification both IETF and markdown parsing.

## 0.3.0 (2023-10-06)


### Features

* specify path to the spec files ([#118](https://github.com/awslabs/duvet/issues/118)) ([ce9325e](https://github.com/awslabs/duvet/commit/ce9325ec7e5352f73a26d4b6a4dde34b58b06de1))


## 0.2.0 (2022-11-16)


### Features

* add basic markdown support ([#84](https://github.com/awslabs/duvet/issues/84)) ([f8ebf29](https://github.com/awslabs/duvet/commit/f8ebf298c6dca3c2a261d6a3fbc3703dd1c6703b))


### Bug Fixes

* remove redundant borrows ([#89](https://github.com/awslabs/duvet/issues/89)) ([0cfc8ce](https://github.com/awslabs/duvet/commit/0cfc8ce88a8a5183a68581fd5824498dbe4e376a))
* handle duplicate markdown section names ([#94](https://github.com/awslabs/duvet/issues/94)) ([5d31dd2](https://github.com/awslabs/duvet/commit/5d31dd21c05f5998b8a4e6c66e18552688a3e788))

## 0.1.1 (2022-10-07)

### Features

* Add type implication ([#16](https://github.com/awslabs/duvet/issues/16)) ([45bd9df](https://github.com/awslabs/duvet/commit/45bd9df437ce1788a9b81b6d4d4ff3895b205eec))

### Bug Fixes

* add word boundary assertions for extracted keywords ([#72](https://github.com/awslabs/duvet/issues/72)) ([02c9245](https://github.com/awslabs/duvet/commit/02c92452158debf1be82c702824689ab01b08aa0))
* finish pattern state machine after iterating lines ([#76](https://github.com/awslabs/duvet/issues/76)) ([7d500ff](https://github.com/awslabs/duvet/commit/7d500ffec0bdeaefb1342645965c655b5fd69eed))
* normalize quotes with indentations ([#79](https://github.com/awslabs/duvet/issues/79)) ([65835f7](https://github.com/awslabs/duvet/commit/65835f7cb45c7a84f9f43d7e348225f954a871a5))
* prefix anchors in spec links ([#68](https://github.com/awslabs/duvet/issues/68)) ([93c7875](https://github.com/awslabs/duvet/commit/93c78754f2adb88b4412030b04719c95963f73a1))
* sort Requirements table ([#82](https://github.com/awslabs/duvet/issues/82)) ([71f6152](https://github.com/awslabs/duvet/commit/71f6152dca7a8649823fcddb5a0cccbecc8b7103))
* use BTreeMap for target data ([#86](https://github.com/awslabs/duvet/issues/86)) ([2ea2336](https://github.com/awslabs/duvet/commit/2ea2336fcdd2db247046320c7f3b7b7f4a397bea))
* panic on file without trailing newline ([002fce8](https://github.com/awslabs/duvet/commit/002fce863d7620526e9500d58f9e1268b824841b))
