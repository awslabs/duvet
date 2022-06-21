import pathlib
import pytest
from duvet.identifiers import AnnotationType, RequirementLevel, RequirementStatus
from duvet.spec_md_parser import MDSpec
from duvet.structures import Annotation, Requirement

pytestmark = [pytest.mark.unit, pytest.mark.local]



def test_mdspec_load():
    MDSpec.load()
    path = pathlib.Path("./duvet-specification").resolve()
    patterns = "compliance/**/*.toml"
    test_report = MDSpec.load(path)
    # Verify one spec is added to the report object
    assert len(test_report.specifications.keys()) == 1
