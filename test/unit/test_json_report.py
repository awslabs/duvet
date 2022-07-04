import copy

import pytest

from duvet._config import ImplConfig
from duvet.identifiers import RequirementLevel, AnnotationType
from duvet.json_report import JSONReport
from duvet.structures import Report, Requirement, Specification, Annotation, Section

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
    return Section("target", "test_target.md#target", 1, 3)


@pytest.fixture
def actual_json():
    return JSONReport()


class TestJSONReport:

    def test_from_annotation(self, actual_json, actual_section, citation):
        actual_index = actual_json.from_annotation(citation)

        assert actual_index == 0
        assert actual_json.annotations == [{'line': -1,
                                            'source': 'code.py',
                                            'target_path': 'test_target.md#target',
                                            'target_section': 'test_target.md#target',
                                            'type': 'CITATION'}]

    def test_from_requirement(self, actual_json, actual_section, actual_requirement):
        data = actual_json.from_requirement(actual_requirement, actual_section)

        # Verify requirement is added to annotation.
        assert actual_json.annotations == [{'line': -1,
                                            'source': 'test_target.md#target$content',
                                            'target_path': 'test_target.md#target$content',
                                            'target_section': 'target',
                                            'type': 'MUST'}]
        assert data == {}

        # def test_from_section(self,actual_json,actual_section):
