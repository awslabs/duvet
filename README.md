## Duvet

A code quality tool to help bound correctness.
By starting from a specification Duvet extracts every RFC 2119 requirement.
Duvet can then use this information to report on a code base.
Duvet can then report on every requirement,
where it is honored in source,
as well as how that source is tested.

## Support
This tool is still in beta.
Interfaces should be considered unstable and may change before the 1.0.0 release.

## Test
First run `make` in the main `duvet` directory to generate the necessary files.
```
cargo test
```

## Build

If there are any changes to the JS
it will also need to be built.
In the `www` directory run `make build`

## Install
```
cargo +stable install --force --path .
````

## Security

See [CONTRIBUTING](CONTRIBUTING.md#security-issue-notifications) for more information.

## License

This project is licensed under the Apache-2.0 License.

