# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0
"""Parse a config file."""
import pathlib
import re
import warnings
from typing import List

import attr
import toml
from attr import define, field

__all__ = ["Config", "DEFAULT_META_STYLE", "DEFAULT_CONTENT_STYLE"]

DEFAULT_META_STYLE = "//="
DEFAULT_CONTENT_STYLE = "//#"


# TODO:  update _config to handle spec.toml # pylint:disable=W0511


@define
class ImplConfig:
    """Implementation container."""

    impl_filenames: List[pathlib.Path] = field(
        init=True,
        default=attr.Factory(list),
        validator=attr.validators.deep_iterable(
            member_validator=attr.validators.instance_of(pathlib.Path),
            iterable_validator=attr.validators.instance_of(List),
        ),
    )
    meta_style: str = DEFAULT_META_STYLE
    content_style: str = DEFAULT_CONTENT_STYLE

    def __attrs_post_init__(self):
        self._check(self.meta_style)
        self._check(self.content_style)
        if self.meta_style == self.content_style:
            raise TypeError("Meta style and Content style of annotation cannot be same.")

    @staticmethod
    def _check(value: str):
        if not isinstance(value, str):
            raise TypeError("AnnotationPrefixes must be string")
        if re.match(r"[\s]+", value):
            raise TypeError("AnnotationPrefixes must not be all whitespace")
        if len(value) < 3:
            raise TypeError("AnnotationPrefixes must have 3 or more characters")


@define
class Config:
    """Duvet configuration container and parser."""

    # This is the directory we kept a record for report generation purpose.
    config_path: pathlib.Path = field(init=True)
    implementation_configs: List[ImplConfig] = field(init=True, default=attr.Factory(list))
    specs: List[pathlib.Path] = field(init=True, default=attr.Factory(list))
    legacy: bool = field(init=True, default=False)
    blob_url: str = field(init=True, default="Github Blob URL Placeholder")
    issue_url: str = field(init=True, default="Github Issue URL Placeholder")

    @classmethod
    def parse(cls, config_file_path: str) -> "Config":
        """Parse a config file."""
        return ConfigParser(pathlib.Path(config_file_path)).extract_config()


@define
class ConfigParser:
    """Parser of config toml file."""

    config_file_path: pathlib.Path

    def extract_config(self) -> Config:
        """Parse a config file."""
        legacy = False
        with open(self.config_file_path, "r", encoding="utf-8") as config_file:
            parsed = toml.load(config_file)
        if "implementation" not in parsed.keys():
            raise ValueError("Implementation Config not found.")
        if "spec" not in parsed.keys():
            raise ValueError("Specification Config not found.")
        if "report" not in parsed.keys():
            raise ValueError("Report Config not found.")
        if "mode" not in parsed.keys():
            pass
        else:
            legacy = parsed.get("mode", {}).get("legacy", False)
        implementation_configs = self._validate_implementation(parsed.get("implementation", {}))
        spec_configs = self._validate_specification(parsed.get("spec", {}))
        # print(implementation_configs)
        return Config(
            self.config_file_path.parent,
            implementation_configs,
            spec_configs,
            legacy,
            parsed.get("report", {}).get("blob", {}).get("url", "Github Blob URL Placeholder"),
            parsed.get("report", {}).get("issue", {}).get("url", "Github Issue URL Placeholder"),
        )

    def _validate_patterns(self, spec: dict, entry_key: str, mode: str) -> List[pathlib.Path]:
        spec_file_list = []
        entry = spec.get(entry_key, {})
        if "patterns" not in entry.keys():
            raise ValueError("Patterns not found in" + mode + " Config " + entry_key)
        for pattern in entry.get("patterns"):
            temp_list = list(self.config_file_path.parent.glob(pattern))
            if len(temp_list) == 0:
                warnings.warn("No files found in pattern " + pattern + " in " + mode)
            else:
                spec_file_list.extend(temp_list)
        return [pathlib.Path(x) for x in spec_file_list]

    def _validate_specification(self, spec: dict) -> List[pathlib.Path]:
        """Validate Config specification files."""
        spec_file_list = []
        for entry_key in spec.keys():
            spec_file_list.extend(self._validate_patterns(spec, entry_key, "Specification"))
        return spec_file_list

    def _validate_implementation(self, impl: dict) -> List[ImplConfig]:
        """Validate Config implementation files."""
        impl_config_list = []
        for entry_key in impl.keys():
            entry = impl.get(entry_key, {})
            impl_file_list = self._validate_patterns(impl, entry_key, "Implementation")
            temp_impl_config = ImplConfig(impl_file_list)
            if "comment-style" in entry.keys():
                comment_style = entry.get("comment-style")
                temp_impl_config = ImplConfig(
                    impl_file_list,
                    comment_style.get("meta", DEFAULT_META_STYLE),
                    comment_style.get("content", DEFAULT_CONTENT_STYLE),
                )
            impl_config_list.append(temp_impl_config)
        return impl_config_list

# //= compliance/duvet-specification.txt#2.3.1
# //# This identifier of meta parts MUST be configurable.

# //= compliance/duvet-specification.txt#2.3.6
# //= type=implication
# //# The default identifier for the content part in software documents MUST be "//#" followed by a single space.

# //= compliance/duvet-specification.txt#2.3.6
# //= type=implication
# //# This identifier of content parts MUST be configurable.

# //= compliance/duvet-specification.txt#2.3.6
# //= type=implication
# //# All content part lines MUST be consecutive.
