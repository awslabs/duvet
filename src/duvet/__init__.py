# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Annotation Parser used by duvet-python."""
import os
from typing import Optional

import click

import duvet._config
import duvet._run_checks

__all__ = ("__version__", "cli")

__version__ = "1.0.0"
_DEBUG = "INPUT_DEBUG"
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
@click.option("-v", "--verbose", count=True)
@click.version_option(version=f"duvet version {__version__}")
def cli(config: Optional[str], verbose: int):
    """Duvet runs checks against specs and implementations."""
    if _DEBUG in os.environ:
        verbose += 1

    if config is None:
        try:
            config = os.environ[_CONFIG_FILE]
        except KeyError as error:
            raise click.exceptions.BadOptionUsage(
                option_name="config",
                message=f"Config file must provided.",
            ) from error
        if not os.path.isfile(config):
            raise click.BadOptionUsage(
                option_name="config",
                message=f"Requested config file '{config}' does not exist or is not a file",
            )

    parsed_config = _config.Config.parse(config)
    success = _run_checks.run(config=parsed_config)
    if not success:
        raise click.ClickException("Checks failed!")


if __name__ == "__main__":
    cli()
