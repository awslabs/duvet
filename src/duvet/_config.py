# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Parse a config file."""
import re
import warnings
from pathlib import Path
from typing import List

import attr
import toml
from attr import define, field

from duvet.exceptions import ConfigError
from duvet.identifiers import DEFAULT_CONTENT_STYLE, DEFAULT_META_STYLE


# TODO:  update _config to handle spec.toml # pylint:disable=W0511


@define
class ImplConfig:
    """Implementation container."""

    impl_filenames: List[Path] = field(
        init=True,
        default=attr.Factory(list),
        validator=attr.validators.deep_iterable(
            member_validator=attr.validators.instance_of(Path),
            iterable_validator=attr.validators.instance_of(List),
        ),
    )
    meta_style: str = DEFAULT_META_STYLE
    content_style: str = DEFAULT_CONTENT_STYLE

    def __attrs_post_init__(self):
        self._check(self.meta_style)
        self._check(self.content_style)
        if self.meta_style == self.content_style:
            raise ConfigError("Meta style and Content style of annotation cannot be same.")

    @staticmethod
    def _check(value: str):
        if not isinstance(value, str):
            raise ConfigError("AnnotationPrefixes must be string")
        if re.match(r"[\s]+", value):
            raise ConfigError("AnnotationPrefixes must not be all whitespace")
        if len(value) < 3:
            raise ConfigError("AnnotationPrefixes must have 3 or more characters")


@define
class Config:
    """Duvet configuration container and parser."""

    # This is the directory we kept a record for report generation purpose.
    config_path: Path = field(init=True)
    specification_path: Path = field(init=True)
    implementation_configs: List[ImplConfig] = field(init=True, default=attr.Factory(list))
    specs: List[Path] = field(init=True, default=attr.Factory(list))
    legacy: bool = field(init=True, default=False)
    blob_url: str = field(init=True, default="Github Blob URL Placeholder")
    issue_url: str = field(init=True, default="Github Issue URL Placeholder")
    specification_path: str = ""

    @classmethod
    def parse(cls, config_file_path: str) -> "Config":
        """Parse a config file."""
        return ConfigParser(Path(config_file_path)).extract_config()


@define
class ConfigParser:
    """Parser of config toml file."""

    config_file_path: Path

    def extract_config(self) -> Config:
        """Parse a config file."""
        legacy = False
        with open(self.config_file_path, "r", encoding="utf-8") as config_file:
            parsed = toml.load(config_file)
        if "implementation" not in parsed.keys():
            raise ConfigError("Implementation Config not found.")
        if "spec" not in parsed.keys():
            raise ConfigError("Specification Config not found.")
        if "report" not in parsed.keys():
            raise ConfigError("Report Config not found.")
        if "mode" not in parsed.keys():
            pass
        else:
            legacy = parsed.get("mode", {}).get("legacy", False)

        specification_path, spec_configs = self._validate_specification(parsed.get("spec", {}))

        implementation_configs = self._validate_implementation(parsed.get("implementation", {}))
        # spec_configs = self._validate_specification(parsed.get("spec", {}))
        specification_path = parsed.get("spec", {}).get("path", self.config_file_path.parent)

        return Config(
            self.config_file_path.parent,
            specification_path,
            implementation_configs,
            spec_configs,
            legacy,
            parsed.get("report", {}).get("blob", {}).get("url", "Github Blob URL Placeholder"),
            parsed.get("report", {}).get("issue", {}).get("url", "Github Issue URL Placeholder"),
        )

    @staticmethod
    def _validate_patterns(directory: Path, spec: dict, entry_key: str, mode: str) -> List[Path]:
        spec_file_list = []
        entry = spec.get(entry_key, {})
        if "patterns" not in entry.keys():
            raise ValueError("Patterns not found in" + mode + " Config " + entry_key)
        for pattern in entry.get("patterns"):
            temp_list = list(directory.glob(pattern))
            if len(temp_list) == 0:
                warnings.warn("No files found in pattern " + pattern + " in " + mode)
            else:
                spec_file_list.extend(temp_list)
        return [Path(x) for x in spec_file_list]

    def _validate_specification(self, spec: dict) -> (Path, list):
        """Validate Config specification files."""

        specifications: list = []
        for entry_key in spec.keys():
            specification_path = self.config_file_path.parent.joinpath(spec.get(entry_key).get("path"), "")
            filenames = ConfigParser._validate_patterns(specification_path, spec, entry_key, "Specification")
            specifications.extend(filenames)
        return specification_path, specifications

    def _validate_implementation(self, impl: dict) -> List[ImplConfig]:
        """Validate Config implementation files."""
        impl_config_list = []
        for entry_key in impl.keys():
            entry = impl.get(entry_key, {})
            impl_file_list = self._validate_patterns(self.config_file_path.parent, impl, entry_key, "Implementation")
            temp_impl_config = ImplConfig(impl_file_list)
            if "comment-style" in entry.keys():
                comment_style = entry.get("comment-style")

                # //= compliance/duvet-specification.txt#2.3.1
                # //= type=implication
                # //# This identifier of meta parts MUST be configurable.

                # //= compliance/duvet-specification.txt#2.3.6
                # //= type=implication
                # //# This identifier of content parts MUST be configurable.

                # //= compliance/duvet-specification.txt#2.3.6
                # //= type=implication
                # //# All content part lines MUST be consecutive.

                temp_impl_config = ImplConfig(
                    impl_file_list,
                    comment_style.get("meta", DEFAULT_META_STYLE),
                    comment_style.get("content", DEFAULT_CONTENT_STYLE),
                )
            impl_config_list.append(temp_impl_config)
        return impl_config_list
