# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""duvet-python."""
import io
import os
import re

from setuptools import find_packages, setup

VERSION_RE = re.compile(r"""__version__ = ['"]([0-9.]+)['"]""")
HERE = os.path.abspath(os.path.dirname(__file__))


def read(*args):
    """Read complete file contents."""
    return io.open(os.path.join(HERE, *args), encoding="utf-8").read()


def get_version():
    """Read the version from this module."""
    init = read("src", "duvet", "identifiers.py")
    return VERSION_RE.search(init).group(1)


def get_requirements():
    """Read the requirements file."""
    raw_requirements = read("requirements.txt")
    requirements = []
    dependencies = []

    for req in raw_requirements.splitlines():
        req = req.strip()
        if not req:
            continue
        elif req.startswith("#"):
            continue
        elif "+" in req:
            dependencies.append(req)
        else:
            requirements.append(req)

    return requirements, dependencies


install_requires, dependency_links = get_requirements()

# noinspection PyTypeChecker
setup(
    name="duvet",
    version=get_version(),
    packages=find_packages("src"),
    package_dir={"": "src"},
    url="https://github.com/awslabs/duvet",
    author="Amazon Web Services",
    author_email="aws-cryptools@amazon.com",
    maintainer="Amazon Web Services",
    description="A code quality tool to help bound correctness."
    " By starting from a specification Duvet extracts every RFC 2119 requirement. "
    " Duvet can then use this information to report on a code base."
    " Duvet can then report on every requirement,"
    " where it is honored in source, as well as how that source is tested.",
    long_description=read("README.md"),
    keywords="duvet duvet aws",
    data_files=["README.md", "CHANGELOG.rst", "LICENSE", "requirements.txt"],
    license="Apache 2.0",
    install_requires=install_requires,
    dependency_links=dependency_links,
    classifiers=[
        "Development Status :: 2 - Pre-Alpha",
        "Intended Audience :: Developers",
        "Natural Language :: English",
        "License :: OSI Approved :: Apache Software License",
        "Programming Language :: Python",
        "Programming Language :: Python :: 3",
        "Programming Language :: Python :: [3.9",
        "Programming Language :: Python :: 3.10]",
        "Programming Language :: Python :: Implementation :: CPython",
        "Topic :: Software Development",
    ],
)
