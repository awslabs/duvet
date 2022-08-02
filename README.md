## Duvet

A code quality tool to help bound correctness.
By starting from a specification Duvet extracts every RFC 2119 requirement.
Duvet can then use this information to report on a code base.
Duvet can then report on every requirement,
where it is honored in source,
as well as how that source is tested.

## Install

```
pyenv exec python setup.py install
```

## Usage

```commandline
duvet -c duvet.toml
```

### Configuration Reference

```toml
[implementation]
[implementation.dfy]
patterns = ["src/**/*.dfy", "test/**/*.dfy", "compliance_exceptions/**/*.txt"]
# no comment-style needed, as Dafny can use the default //#, //=
[implementation.py]
patterns = ["src/**/*.py", "test/**/*.py"]
comment-style = { meta = "# //=", content = "# //#" }
[spec]
[spec.markdown]
patterns = ["project-specification/**/*.md"]
[spec.toml]
patterns = ["project-specification/**/*.toml"]
[report]
[report.blob]
url = "https://github.com/aws/aws-encryption-sdk-dafny/blob/"
[report.issue]
url = "https://github.com/aws/aws-encryption-sdk-dafny/issues"
[mode]
legacy = true
```

## Development

### Prerequisites

* Required

    * Python 3.9+
    * [`tox`](http://tox.readthedocs.io/): We use tox to drive all of our testing and package management behavior.
      Any tests that you want to run should be run using tox.

* Optional

    * [`pyenv`](https://github.com/pyenv/pyenv): If you want to test against multiple versions of Python and are on
      Linux or macOS,
      we recommend using pyenv to manage your Python runtimes.
    * [`tox-pyenv`](https://pypi.org/project/tox-pyenv/): Plugin for tox that enables it to use pyenv runtimes.
    * [`detox`](https://pypi.org/project/detox/): Parallel plugin for tox. Useful for running a lot of test environments
      quickly.

### Build

If there are any changes to the JS
it will also need to be built.
In the `www` directory run `make build`

### Testing

Testing is done with `tox`. There are various `tox` environments specified in the `tox.ini`.

For example, to run the Python 3.9 local tests:

```bash
tox -e py39-local
```

To run `autoformat`:

```bash
tox -e autoformat
```

To run all the `tox` tests, just run `tox` with no arguments.

## Security

See [CONTRIBUTING](CONTRIBUTING.md#security-issue-notifications) for more information.

## License

This project is licensed under the Apache-2.0 License.

