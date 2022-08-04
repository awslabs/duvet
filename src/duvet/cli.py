# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""cli used by duvet-python."""
import os
from typing import Optional

import click

from duvet._config import Config
from duvet._run_checks import run
from duvet.identifiers import __version__

_CONFIG_FILE = "INPUT_CONFIG-FILE"


@click.command()
@click.option(
    "-c",
    "--config",
    default=None,
    required=False,
    type=click.Path(exists=True, file_okay=True, dir_okay=False, resolve_path=True, readable=True),
    help="Path to config file",
)
@click.version_option(version=f"duvet version {__version__}")
def cli(config: Optional[str]) -> int:
    """Duvet runs checks against specs and implementations."""

    if config is None:
        try:
            config = os.environ[_CONFIG_FILE]
        except KeyError as error:
            raise click.exceptions.BadOptionUsage(
                option_name="config",
                message="Config file must be provided.",
            ) from error
        if not os.path.isfile(config):
            raise click.BadOptionUsage(
                option_name="config",
                message=f"Requested config file '{config}' does not exist or is not a file",
            )

    parsed_config = Config.parse(config)
    # click.echo(parsed_config)
    success = run(config=parsed_config)
    if not success:
        # //= compliance/duvet-specification.txt#2.6.2
        # //= type=implication
        # //# Duvet MUST NOT return "0" for Fail.
        # //= compliance/duvet-specification.txt#2.6.2
        # //= type=implication
        # //# Duvet SHOULD print a success message.
        click.echo("Duvet: FAIL. Incomplete MUST requirements found.")
        return -1
    else:
        # //= compliance/duvet-specification.txt#2.6.2
        # //= type=implication
        # //# Duvet MUST return "0" for Pass.
        # //= compliance/duvet-specification.txt#2.6.2
        # //= type=implication
        # //# Duvet SHOULD print a failure message.
        click.echo("Duvet: PASS. Congratulations :)")
        return 0


if __name__ == "__main__":
    cli()  # pylint:disable=E1120