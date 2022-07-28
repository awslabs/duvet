# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""cli used by duvet-python."""
import logging
import os
from typing import Optional

import click  # type : ignore[import]

from duvet._config import Config
from duvet._run_checks import run
from duvet.identifiers import __version__

_DEBUG = "INPUT_DEBUG"
_CONFIG_FILE = "INPUT_CONFIG-FILE"
_USAGE_URL: str = "https://github.com/awslabs/duvet/tree/feat-run-checks#usage"


@click.command()
@click.option(
    "-c",
    "--config",
    default=None,
    required=False,
    type=click.Path(exists=True, file_okay=True, dir_okay=False, resolve_path=True, readable=True),
    help=f"Path to config file. You can find an example on {_USAGE_URL}",
)
@click.option("-v", "--verbose", default=0, required=False, type=int)
@click.version_option(version=f"{__version__}", message=f"Duvet, version {__version__}\nDocumentation at {_USAGE_URL}")
def cli(config: Optional[str], verbose: int = 0):
    """Duvet runs checks against specs and implementations."""

    # Handle config option.
    if config is None:
        try:
            config = os.environ[_CONFIG_FILE]
        except KeyError as error:
            raise click.exceptions.BadOptionUsage(
                option_name="config",
                message="Config file must provided.",
            ) from error
        if not os.path.isfile(config):
            raise click.BadOptionUsage(
                option_name="config",
                message=f"Requested config file '{config}' does not exist or is not a file",
            )

    # Handle verbose option for logging.
    logger = logging.getLogger(__name__)
    if _DEBUG in os.environ:
        verbose += 10

    logger.setLevel(verbose)
    click.echo(f"Setting logger level to {logger.level}")

    # Parse configuration file.
    parsed_config = Config.parse(config)

    success: bool = run(config=parsed_config)
    if not success:
        # //= compliance/duvet-specification.txt#2.6.2
        # //= type=implication
        # //# Duvet MUST NOT return "0" for Fail.
        # //= compliance/duvet-specification.txt#2.6.2
        # //= type=implication
        # //# Duvet SHOULD print a failure message.
        click.echo("Duvet: FAIL. Incomplete MUST requirements found.")
        return -1
    else:
        # //= compliance/duvet-specification.txt#2.6.2
        # //= type=implication
        # //# Duvet MUST return "0" for Pass.
        # //= compliance/duvet-specification.txt#2.6.2
        # //= type=implication
        # //# Duvet SHOULD print a success message.
        click.echo("Duvet: PASS. Congratulations :)")
        return 0
