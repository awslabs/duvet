import copy

import pytest

from duvet.identifiers import AnnotationType, RequirementLevel
from duvet.json_report import JSONReport
from duvet.structures import Annotation, Report, Requirement, Section, Specification

pytestmark = [pytest.mark.local, pytest.mark.unit]

VALID_KWARGS: dict = {
    "target": "test_target.md#target",
    "type": AnnotationType.CITATION,
    "start_line": 1,
    "end_line": 2,
    "reason": None,
    "content": "content",
    "uri": "test_target.md#target$content",
    "source": "code.py",
}

REF_STATUS: dict = {
    "spec": bool,
    "citation": bool,
    "implication": bool,
    "test": bool,
    "exception": bool,
    "todo": bool,
    "level": RequirementLevel,
}


def _update_valid_kwargs(updates: dict) -> dict:
    rtn = copy.deepcopy(VALID_KWARGS)
    rtn.update(updates)
    return rtn


def _update_ref(updates: dict) -> dict:
    rtn = copy.deepcopy(REF_STATUS)
    rtn.update(updates)
    return rtn


INVALID_KWARGS = _update_valid_kwargs(
    {"target": "new_test_target.md#new-target", "uri": "new_test_target.md#target$content"}
)


def _help_assert_annotation(annotation: Annotation, kwargs: dict):
    assert annotation.target == kwargs["target"]
    assert annotation.type == kwargs["type"]
    assert annotation.content == kwargs["content"]
    assert annotation.start_line == kwargs["start_line"]
    assert annotation.end_line == kwargs["end_line"]
    assert annotation.uri == kwargs["uri"]
    assert annotation.source == kwargs["source"]


@pytest.fixture
def actual_requirement() -> Requirement:
    return Requirement(RequirementLevel.MUST, "content", "test_target.md#target$content")


@pytest.fixture
def actual_specification() -> Specification:
    return Specification("target", "test_target.md")


@pytest.fixture
def actual_report() -> Report:
    return Report()


@pytest.fixture
def citation() -> Annotation:
    return Annotation(**VALID_KWARGS)


@pytest.fixture
def actual_section() -> Section:
    section = Section("target", "test_target.md#target", 1, 3)
    section.lines = ["1. target", "content"]
    return section


@pytest.fixture
def actual_json(actual_report: Report):
    return JSONReport(actual_report)


class TestJSONReport:
    def test_from_annotation(self, actual_json, actual_section, citation):
        actual_index = actual_json._process_annotation(citation)

        assert actual_index == 0
        assert actual_json.annotations == [
            {
                "line": 1,
                "source": "code.py",
                "target_path": "test_target.md",
                "target_section": "target",
                "type": "CITATION",
            }
        ]

    def test_from_requirement(self, actual_json, actual_section, actual_requirement):
        actual_index = actual_json._process_requirement(actual_requirement, actual_section, [])

        # Verify requirement is added to annotation.
        assert actual_json.annotations == [
            {
                "comment": "content",
                "level": "MUST",
                "source": "test_target.md",
                "target_path": "test_target.md",
                "target_section": "target",
                "type": "SPEC",
            }
        ]

        assert actual_index == 0

    def test_from_section(self, actual_json, actual_section):
        assert actual_json._process_section(actual_section) == {
            "id": "target",
            "lines": ["1. target", "content"],
            "title": "target",
        }

    def test_from_sections(self, actual_json, actual_specification, actual_section, actual_requirement):
        actual_section.add_requirement(actual_requirement)
        actual_specification.add_section(actual_section)
        sections, requirements = actual_json._process_sections(actual_specification.sections)

        assert len(sections) == 3
        assert len(requirements) == 1

    def test_from_specification(self, actual_json, actual_specification, actual_section, actual_requirement):
        # Setup specification for test.
        actual_section.add_requirement(actual_requirement)
        actual_specification.add_section(actual_section)

        actual_json._process_specification(actual_specification)
        specification_dict = actual_json.specifications.get(actual_specification.source)

        assert len(specification_dict.get("sections")) == 3
        assert len(specification_dict.get("requirements")) == 1
